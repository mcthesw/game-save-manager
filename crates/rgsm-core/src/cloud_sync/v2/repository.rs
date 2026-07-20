use std::sync::Arc;

use async_trait::async_trait;
use opendal::{ErrorKind, Operator};
use thiserror::Error;
use tokio::sync::Mutex;

use super::{CloudManifest, DeletionRegistryError, DeletionRegistryRepository, ManifestError};

#[async_trait]
pub trait ManifestTransport: Send + Sync {
    async fn read(&self) -> Result<Option<Vec<u8>>, opendal::Error>;
    async fn write(&self, bytes: &[u8]) -> Result<(), opendal::Error>;
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

    pub async fn load(&self) -> Result<CloudManifest, ManifestRepositoryError> {
        let manifest = match self.transport.read().await? {
            Some(bytes) => serde_json::from_slice(&bytes)?,
            None => CloudManifest::default(),
        };
        manifest.validate()?;
        Ok(manifest)
    }

    /// Apply one idempotent mutation to the latest remote Manifest.
    ///
    /// Each retry re-reads and re-applies the mutation. In-process writers are
    /// serialized; cross-process safety is bounded by read-back visibility.
    pub async fn mutate<F>(&self, mutation: F) -> Result<CloudManifest, ManifestRepositoryError>
    where
        F: Fn(&mut CloudManifest) -> Result<(), ManifestError>,
    {
        let _guard = self.writer_lock.lock().await;
        for _ in 0..self.max_attempts {
            let mut manifest = self.load().await?;
            let previous = manifest.clone();
            mutation(&mut manifest)?;
            self.reject_deleted_game_changes(&previous, &manifest)
                .await?;
            manifest.revision = manifest
                .revision
                .checked_add(1)
                .ok_or(ManifestRepositoryError::RevisionOverflow)?;
            manifest.validate()?;
            let expected = serde_json::to_vec_pretty(&manifest)?;
            self.transport.write(&expected).await?;
            if self.transport.read().await?.as_deref() == Some(expected.as_slice()) {
                return Ok(manifest);
            }
        }
        Err(ManifestRepositoryError::RetryExhausted {
            attempts: self.max_attempts,
        })
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
            .load()
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
    #[error("Cloud Manifest revision overflow")]
    RevisionOverflow,
    #[error("Cloud Manifest read-back verification failed after {attempts} attempts")]
    RetryExhausted { attempts: usize },
    #[error("Game {0} has been permanently deleted")]
    DeletedGame(String),
    #[error(transparent)]
    DeletionRegistry(#[from] DeletionRegistryError),
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use super::*;

    struct FakeState {
        bytes: Option<Vec<u8>>,
        overwrite_writes: usize,
    }

    struct FakeTransport {
        state: StdMutex<FakeState>,
    }

    impl FakeTransport {
        fn new(overwrite_writes: usize) -> Self {
            Self {
                state: StdMutex::new(FakeState {
                    bytes: None,
                    overwrite_writes,
                }),
            }
        }
    }

    #[async_trait]
    impl ManifestTransport for FakeTransport {
        async fn read(&self) -> Result<Option<Vec<u8>>, opendal::Error> {
            Ok(self.state.lock().unwrap().bytes.clone())
        }

        async fn write(&self, bytes: &[u8]) -> Result<(), opendal::Error> {
            let mut state = self.state.lock().unwrap();
            if state.overwrite_writes > 0 {
                state.overwrite_writes -= 1;
                let external = CloudManifest {
                    revision: 100 + state.overwrite_writes as u64,
                    ..CloudManifest::default()
                };
                state.bytes = Some(serde_json::to_vec_pretty(&external).unwrap());
            } else {
                state.bytes = Some(bytes.to_vec());
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn mutation_reapplies_after_read_back_mismatch() {
        let repository = CloudManifestRepository::with_transport(FakeTransport::new(1), 3);

        let stored = repository
            .mutate(|manifest| {
                manifest.game_mut("game");
                Ok(())
            })
            .await
            .unwrap();

        assert!(stored.games.contains_key("game"));
        assert!(stored.revision > 100);
    }

    #[tokio::test]
    async fn bounded_mismatch_returns_typed_error() {
        let repository = CloudManifestRepository::with_transport(FakeTransport::new(10), 2);

        let result = repository.mutate(|_| Ok(())).await;

        assert!(matches!(
            result,
            Err(ManifestRepositoryError::RetryExhausted { attempts: 2 })
        ));
    }

    #[tokio::test]
    async fn malformed_remote_manifest_fails_closed() {
        let transport = FakeTransport::new(0);
        transport.state.lock().unwrap().bytes = Some(b"{broken".to_vec());
        let repository = CloudManifestRepository::with_transport(transport, 2);

        assert!(matches!(
            repository.load().await,
            Err(ManifestRepositoryError::Serialization(_))
        ));
    }
}
