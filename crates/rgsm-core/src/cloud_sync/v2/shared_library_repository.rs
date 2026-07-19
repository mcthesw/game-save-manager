use std::sync::Arc;

use thiserror::Error;
use tokio::sync::Mutex;

use super::{ManifestTransport, OpenDalManifestTransport, SHARED_LIBRARY_PATH};
use crate::config::{OwnershipError, SharedLibrary};

pub struct SharedLibraryRepository<T: ManifestTransport> {
    transport: Arc<T>,
    writer_lock: Mutex<()>,
    max_attempts: usize,
}

impl SharedLibraryRepository<OpenDalManifestTransport> {
    pub fn new(operator: opendal::Operator, max_attempts: usize) -> Self {
        Self::with_transport(
            OpenDalManifestTransport::new(operator, SHARED_LIBRARY_PATH),
            max_attempts,
        )
    }
}

impl<T: ManifestTransport> SharedLibraryRepository<T> {
    pub fn with_transport(transport: T, max_attempts: usize) -> Self {
        Self {
            transport: Arc::new(transport),
            writer_lock: Mutex::new(()),
            max_attempts: max_attempts.max(1),
        }
    }

    pub async fn load(&self) -> Result<SharedLibrary, SharedLibraryRepositoryError> {
        let bytes = self
            .transport
            .read()
            .await?
            .ok_or(SharedLibraryRepositoryError::Missing)?;
        let library: SharedLibrary = serde_json::from_slice(&bytes)?;
        library.validate()?;
        Ok(library)
    }

    /// Compare-and-replace one complete Shared Library and verify provider
    /// visibility. A changed remote value fails closed instead of merging or
    /// overwriting unrelated Game definitions.
    pub async fn compare_replace(
        &self,
        expected: &SharedLibrary,
        accepted: &SharedLibrary,
    ) -> Result<SharedLibrary, SharedLibraryRepositoryError> {
        expected.validate()?;
        accepted.validate()?;
        let _guard = self.writer_lock.lock().await;
        let accepted_bytes = serde_json::to_vec_pretty(accepted)?;
        for _ in 0..self.max_attempts {
            let current = self.load().await?;
            if current == *accepted {
                return Ok(current);
            }
            if current != *expected {
                return Err(SharedLibraryRepositoryError::Stale);
            }
            self.transport.write(&accepted_bytes).await?;
            if self.transport.read().await?.as_deref() == Some(accepted_bytes.as_slice()) {
                return Ok(accepted.clone());
            }
        }
        Err(SharedLibraryRepositoryError::RetryExhausted {
            attempts: self.max_attempts,
        })
    }
}

#[derive(Debug, Error)]
pub enum SharedLibraryRepositoryError {
    #[error("Shared Library object is missing")]
    Missing,
    #[error("Shared Library changed before this update could be committed")]
    Stale,
    #[error("Shared Library read-back verification failed after {attempts} attempts")]
    RetryExhausted { attempts: usize },
    #[error("Shared Library transport error: {0}")]
    Transport(#[from] opendal::Error),
    #[error("Shared Library serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error(transparent)]
    Ownership(#[from] OwnershipError),
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
