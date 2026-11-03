use std::sync::Arc;

use thiserror::Error;
use tokio::sync::Mutex;

use super::metadata_read::read_complete_json;
use super::read_round::{cached_read, invalidate};
use super::{
    DeletionRegistryError, DeletionRegistryRepository, ManifestTransport, OpenDalManifestTransport,
    SHARED_LIBRARY_PATH,
};
use crate::config::{OwnershipError, SharedLibrary};

pub struct SharedLibraryRepository<T: ManifestTransport> {
    transport: Arc<T>,
    deletion_operator: Option<opendal::Operator>,
    writer_lock: Mutex<()>,
    max_attempts: usize,
}

impl SharedLibraryRepository<OpenDalManifestTransport> {
    pub fn new(operator: opendal::Operator, max_attempts: usize) -> Self {
        let transport = OpenDalManifestTransport::new(operator.clone(), SHARED_LIBRARY_PATH);
        Self {
            transport: Arc::new(transport),
            deletion_operator: Some(operator),
            writer_lock: Mutex::new(()),
            max_attempts: max_attempts.max(1),
        }
    }
}

impl<T: ManifestTransport> SharedLibraryRepository<T> {
    pub fn with_transport(transport: T, max_attempts: usize) -> Self {
        Self {
            transport: Arc::new(transport),
            deletion_operator: None,
            writer_lock: Mutex::new(()),
            max_attempts: max_attempts.max(1),
        }
    }

    /// Reuses the answer already fetched in this reconciliation round; writes
    /// compare against [`Self::load_fresh`].
    pub async fn load(&self) -> Result<SharedLibrary, SharedLibraryRepositoryError> {
        let bytes = cached_read(self.transport.cache_key(), || self.read_complete()).await?;
        Self::decode(bytes)
    }

    pub async fn load_fresh(&self) -> Result<SharedLibrary, SharedLibraryRepositoryError> {
        Self::decode(self.read_complete().await?)
    }

    async fn read_complete(&self) -> Result<Option<Vec<u8>>, opendal::Error> {
        read_complete_json(|| self.transport.read(), self.max_attempts).await
    }

    fn decode(bytes: Option<Vec<u8>>) -> Result<SharedLibrary, SharedLibraryRepositoryError> {
        let bytes = bytes.ok_or(SharedLibraryRepositoryError::Missing)?;
        let library: SharedLibrary = serde_json::from_slice(&bytes)?;
        library.validate()?;
        Ok(library)
    }

    /// Compare-and-replace one complete Shared Library. A changed remote value
    /// fails closed instead of merging or overwriting unrelated Game
    /// definitions. A successful provider write is trusted without a read-back.
    pub async fn compare_replace(
        &self,
        expected: &SharedLibrary,
        accepted: &SharedLibrary,
    ) -> Result<SharedLibrary, SharedLibraryRepositoryError> {
        expected.validate()?;
        accepted.validate()?;
        let _guard = self.writer_lock.lock().await;
        let current = self.load_fresh().await?;
        if current == *accepted {
            return Ok(current);
        }
        if current != *expected {
            return Err(SharedLibraryRepositoryError::Stale);
        }
        self.reject_deleted_game_changes(&current, accepted).await?;
        let written = self
            .transport
            .write(&serde_json::to_vec_pretty(accepted)?)
            .await;
        if let Some(key) = self.transport.cache_key() {
            invalidate(key);
        }
        written?;
        Ok(accepted.clone())
    }

    async fn reject_deleted_game_changes(
        &self,
        expected: &SharedLibrary,
        accepted: &SharedLibrary,
    ) -> Result<(), SharedLibraryRepositoryError> {
        let Some(operator) = &self.deletion_operator else {
            return Ok(());
        };
        let registry = DeletionRegistryRepository::new(operator.clone(), self.max_attempts)
            .load_fresh()
            .await?;
        for game_id in registry.deleted_games.keys() {
            let previous = expected
                .games
                .iter()
                .find(|game| game.storage_key == *game_id);
            let next = accepted
                .games
                .iter()
                .find(|game| game.storage_key == *game_id);
            match (previous, next) {
                (None, Some(_)) => {
                    return Err(SharedLibraryRepositoryError::DeletedGame(game_id.clone()));
                }
                (Some(before), Some(after)) if before != after => {
                    return Err(SharedLibraryRepositoryError::DeletedGame(game_id.clone()));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum SharedLibraryRepositoryError {
    #[error("{}", rust_i18n::t!("error.cloud_library_damaged"))]
    Missing,
    #[error("{}", rust_i18n::t!("error.cloud_library_changed"))]
    Stale,
    #[error("Shared Library transport error: {0}")]
    Transport(#[from] opendal::Error),
    #[error("Shared Library serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error(transparent)]
    Ownership(#[from] OwnershipError),
    #[error("Game {0} has been permanently deleted")]
    DeletedGame(String),
    #[error(transparent)]
    DeletionRegistry(#[from] DeletionRegistryError),
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use async_trait::async_trait;

    use super::*;
    use crate::config::{SharedGame, SharedSnapshotRetentionPolicy, V2_CONFIG_SCHEMA_VERSION};

    struct FakeTransport {
        bytes: StdMutex<Option<Vec<u8>>>,
        hide_writes: StdMutex<usize>,
    }

    #[async_trait]
    impl ManifestTransport for FakeTransport {
        async fn read(&self) -> Result<Option<Vec<u8>>, opendal::Error> {
            Ok(self.bytes.lock().unwrap().clone())
        }

        async fn write(&self, bytes: &[u8]) -> Result<(), opendal::Error> {
            let mut hidden = self.hide_writes.lock().unwrap();
            if *hidden > 0 {
                *hidden -= 1;
            } else {
                *self.bytes.lock().unwrap() = Some(bytes.to_vec());
            }
            Ok(())
        }
    }

    fn library(limit: Option<u32>) -> SharedLibrary {
        SharedLibrary {
            schema_version: V2_CONFIG_SCHEMA_VERSION,
            games: vec![SharedGame {
                name: "Example".into(),
                storage_key: "example".into(),
                save_units: Vec::new(),
                next_save_unit_id: 0,
                ludusavi_meta: None,
                snapshot_retention: limit.map(|automatic_snapshots_per_branch| {
                    SharedSnapshotRetentionPolicy {
                        automatic_snapshots_per_branch,
                    }
                }),
            }],
        }
    }

    fn repository(
        initial: &SharedLibrary,
        hide_writes: usize,
        attempts: usize,
    ) -> SharedLibraryRepository<FakeTransport> {
        SharedLibraryRepository::with_transport(
            FakeTransport {
                bytes: StdMutex::new(Some(serde_json::to_vec_pretty(initial).unwrap())),
                hide_writes: StdMutex::new(hide_writes),
            },
            attempts,
        )
    }

    #[tokio::test]
    async fn compare_replace_retries_provider_visibility() {
        let expected = library(None);
        let accepted = library(Some(3));
        let repository = repository(&expected, 1, 3);

        assert_eq!(
            repository
                .compare_replace(&expected, &accepted)
                .await
                .unwrap(),
            accepted
        );
    }

    #[tokio::test]
    async fn shared_library_read_waits_for_an_in_progress_overwrite() {
        let root = temp_dir::TempDir::new().unwrap();
        let operator = opendal::Operator::new(
            opendal::services::Fs::default().root(root.path().to_string_lossy().as_ref()),
        )
        .unwrap()
        .finish();
        operator
            .write(SHARED_LIBRARY_PATH, b"{\"games\":".to_vec())
            .await
            .unwrap();
        let writer = operator.clone();
        let write = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            writer
                .write(
                    SHARED_LIBRARY_PATH,
                    serde_json::to_vec(&library(None)).unwrap(),
                )
                .await
                .unwrap();
        });

        let result = SharedLibraryRepository::new(operator, 10).load().await;
        write.await.unwrap();
        assert_eq!(result.unwrap(), library(None));
    }

    #[tokio::test]
    async fn compare_replace_never_overwrites_a_changed_library() {
        let expected = library(None);
        let changed = library(Some(5));
        let repository = repository(&changed, 0, 3);

        assert!(matches!(
            repository
                .compare_replace(&expected, &library(Some(3)))
                .await,
            Err(SharedLibraryRepositoryError::Stale)
        ));
        assert_eq!(repository.load().await.unwrap(), changed);
    }
}
