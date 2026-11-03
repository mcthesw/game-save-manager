use std::sync::Arc;

use async_trait::async_trait;
use opendal::{ErrorKind, Operator};
use thiserror::Error;
use tokio::sync::Mutex;

use super::metadata_read::read_complete_json;
use super::read_round::{cached_read, invalidate};
use super::{CloudManifest, DeletionRegistryError, DeletionRegistryRepository, ManifestError};

#[async_trait]
pub trait ManifestTransport: Send + Sync {
    async fn read(&self) -> Result<Option<Vec<u8>>, opendal::Error>;
    async fn write(&self, bytes: &[u8]) -> Result<(), opendal::Error>;

    /// Identifies the object for the per-round read cache; `None` opts out.
    fn cache_key(&self) -> Option<&str> {
        None
    }
}

pub struct OpenDalManifestTransport {
    operator: Operator,
    object_key: String,
}

impl OpenDalManifestTransport {
    pub fn new(operator: Operator, object_key: impl Into<String>) -> Self {
        Self {
            operator,
            object_key: object_key.into(),
        }
    }
}

#[async_trait]
impl ManifestTransport for OpenDalManifestTransport {
    async fn read(&self) -> Result<Option<Vec<u8>>, opendal::Error> {
        match self.operator.read(&self.object_key).await {
            Ok(bytes) => Ok(Some(bytes.to_vec())),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn write(&self, bytes: &[u8]) -> Result<(), opendal::Error> {
        self.operator
            .write(&self.object_key, bytes.to_vec())
            .await?;
        Ok(())
    }

    fn cache_key(&self) -> Option<&str> {
        Some(&self.object_key)
    }
}

pub struct CloudManifestRepository<T: ManifestTransport> {
    transport: Arc<T>,
    deletion_operator: Option<Operator>,
    writer_lock: Mutex<()>,
    max_attempts: usize,
}

impl CloudManifestRepository<OpenDalManifestTransport> {
    pub fn new(operator: Operator, object_key: impl Into<String>, max_attempts: usize) -> Self {
        let transport = OpenDalManifestTransport::new(operator.clone(), object_key);
        Self {
            transport: Arc::new(transport),
            deletion_operator: Some(operator),
            writer_lock: Mutex::new(()),
            max_attempts: max_attempts.max(1),
        }
    }
}

impl<T: ManifestTransport> CloudManifestRepository<T> {
    pub fn with_transport(transport: T, max_attempts: usize) -> Self {
        Self {
            transport: Arc::new(transport),
            deletion_operator: None,
            writer_lock: Mutex::new(()),
            max_attempts: max_attempts.max(1),
        }
    }

    /// Read the Manifest, reusing the answer already fetched in this
    /// reconciliation round. Anything that decides a write must use
    /// [`Self::load_fresh`].
    pub async fn load(&self) -> Result<CloudManifest, ManifestRepositoryError> {
        let bytes = cached_read(self.transport.cache_key(), || self.read_complete()).await?;
        Self::decode(bytes)
    }

    pub async fn load_fresh(&self) -> Result<CloudManifest, ManifestRepositoryError> {
        Self::decode(self.read_complete().await?)
    }

    async fn read_complete(&self) -> Result<Option<Vec<u8>>, opendal::Error> {
        read_complete_json(|| self.transport.read(), self.max_attempts).await
    }

    fn decode(bytes: Option<Vec<u8>>) -> Result<CloudManifest, ManifestRepositoryError> {
        let manifest: CloudManifest = match bytes {
            Some(bytes) => serde_json::from_slice(&bytes)?,
            // A bound library always has its Manifest; treating absence as an
            // empty one would let the next write erase every published Snapshot.
            None => return Err(ManifestRepositoryError::Missing),
        };
        manifest.validate()?;
        Ok(manifest)
    }

    /// Apply one idempotent mutation to the latest remote Manifest.
    ///
    /// In-process writers are serialized. A successful provider write is
    /// trusted without reading the object back: a read-back only narrows the
    /// window in which a concurrent device could overwrite the write unseen,
    /// and costs one more request on every publish.
    pub async fn mutate<F>(&self, mutation: F) -> Result<CloudManifest, ManifestRepositoryError>
    where
        F: Fn(&mut CloudManifest) -> Result<(), ManifestError>,
    {
        let _guard = self.writer_lock.lock().await;
        let mut manifest = self.load_fresh().await?;
        let previous = manifest.clone();
        mutation(&mut manifest)?;
        self.reject_deleted_game_changes(&previous, &manifest)
            .await?;
        manifest.revision = manifest
            .revision
            .checked_add(1)
            .ok_or(ManifestRepositoryError::RevisionOverflow)?;
        manifest.validate()?;
        let bytes = serde_json::to_vec_pretty(&manifest)?;
        let written = self.transport.write(&bytes).await;
        if let Some(key) = self.transport.cache_key() {
            invalidate(key);
        }
        written?;
        Ok(manifest)
    }

    async fn reject_deleted_game_changes(
        &self,
        previous: &CloudManifest,
        accepted: &CloudManifest,
    ) -> Result<(), ManifestRepositoryError> {
        let Some(operator) = &self.deletion_operator else {
            return Ok(());
        };
        let registry = DeletionRegistryRepository::new(operator.clone(), self.max_attempts)
            .load_fresh()
            .await?;
        for game_id in registry.deleted_games.keys() {
            match (previous.games.get(game_id), accepted.games.get(game_id)) {
                (None, Some(_)) => {
                    return Err(ManifestRepositoryError::DeletedGame(game_id.clone()));
                }
                (Some(before), Some(after)) if before != after => {
                    return Err(ManifestRepositoryError::DeletedGame(game_id.clone()));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ManifestRepositoryError {
    #[error("Cloud Manifest transport error: {0}")]
    Transport(#[from] opendal::Error),
    #[error("Cloud Manifest serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("{}", rust_i18n::t!("error.cloud_library_damaged"))]
    Missing,
    #[error("Cloud Manifest revision overflow")]
    RevisionOverflow,
    #[error("Game {0} has been permanently deleted")]
    DeletedGame(String),
    #[error(transparent)]
    DeletionRegistry(#[from] DeletionRegistryError),
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct FakeTransport {
        bytes: StdMutex<Option<Vec<u8>>>,
        reads: AtomicUsize,
    }

    impl FakeTransport {
        fn new() -> Self {
            Self {
                bytes: StdMutex::new(Some(
                    serde_json::to_vec_pretty(&CloudManifest::default()).unwrap(),
                )),
                reads: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl ManifestTransport for FakeTransport {
        async fn read(&self) -> Result<Option<Vec<u8>>, opendal::Error> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(self.bytes.lock().unwrap().clone())
        }

        async fn write(&self, bytes: &[u8]) -> Result<(), opendal::Error> {
            *self.bytes.lock().unwrap() = Some(bytes.to_vec());
            Ok(())
        }

        fn cache_key(&self) -> Option<&str> {
            Some("manifest")
        }
    }

    #[tokio::test]
    async fn a_mutation_reads_the_manifest_once_and_trusts_the_write() {
        let repository = CloudManifestRepository::with_transport(FakeTransport::new(), 3);

        let stored = repository
            .mutate(|manifest| {
                manifest.game_mut("game");
                Ok(())
            })
            .await
            .unwrap();

        assert!(stored.games.contains_key("game"));
        assert_eq!(repository.transport.reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_round_reuses_loads_but_never_a_write_decision_or_a_stale_write() {
        let repository = CloudManifestRepository::with_transport(FakeTransport::new(), 3);
        super::super::with_read_round(async {
            repository.load().await.unwrap();
            repository.load().await.unwrap();
            assert_eq!(repository.transport.reads.load(Ordering::SeqCst), 1);

            repository.load_fresh().await.unwrap();
            assert_eq!(repository.transport.reads.load(Ordering::SeqCst), 2);

            let written = repository
                .mutate(|manifest| {
                    manifest.game_mut("game");
                    Ok(())
                })
                .await
                .unwrap();
            assert_eq!(repository.load().await.unwrap(), written);
        })
        .await;
    }

    #[tokio::test]
    async fn manifest_read_waits_for_an_in_progress_overwrite() {
        let root = temp_dir::TempDir::new().unwrap();
        let operator = Operator::new(
            opendal::services::Fs::default().root(root.path().to_string_lossy().as_ref()),
        )
        .unwrap()
        .finish();
        operator
            .write("manifest.json", Vec::<u8>::new())
            .await
            .unwrap();
        let writer = operator.clone();
        let write = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let manifest = CloudManifest {
                revision: 7,
                ..CloudManifest::default()
            };
            writer
                .write("manifest.json", serde_json::to_vec(&manifest).unwrap())
                .await
                .unwrap();
        });

        let result = CloudManifestRepository::new(operator, "manifest.json", 10)
            .load()
            .await;
        write.await.unwrap();
        assert_eq!(result.unwrap().revision, 7);
    }

    #[tokio::test]
    async fn an_incomplete_manifest_is_not_treated_as_an_empty_library() {
        let transport = FakeTransport::new();
        *transport.bytes.lock().unwrap() = Some(Vec::new());
        let repository = CloudManifestRepository::with_transport(transport, 2);
        assert!(matches!(
            repository.load().await,
            Err(ManifestRepositoryError::Serialization(_))
        ));
    }

    #[tokio::test]
    async fn a_missing_manifest_is_reported_and_never_recreated_by_a_mutation() {
        let transport = FakeTransport::new();
        *transport.bytes.lock().unwrap() = None;
        let repository = CloudManifestRepository::with_transport(transport, 2);

        assert!(matches!(
            repository.load().await,
            Err(ManifestRepositoryError::Missing)
        ));
        assert!(matches!(
            repository.mutate(|_| Ok(())).await,
            Err(ManifestRepositoryError::Missing)
        ));
        assert!(repository.transport.read().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn malformed_remote_manifest_fails_closed() {
        let transport = FakeTransport::new();
        *transport.bytes.lock().unwrap() = Some(b"{broken".to_vec());
        let repository = CloudManifestRepository::with_transport(transport, 2);

        assert!(matches!(
            repository.load().await,
            Err(ManifestRepositoryError::Serialization(_))
        ));
    }
}
