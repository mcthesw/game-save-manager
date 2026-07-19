use std::path::Path;

use async_trait::async_trait;
use opendal::Operator;
use thiserror::Error;

use super::{CloudManifestRepository, DeletionKind, ManifestRepositoryError, ManifestTransport};

#[async_trait]
pub trait ArchiveDeletionBackend: Send + Sync {
    async fn remove_local(&self, path: &Path) -> Result<(), ArchiveDeletionError>;
    async fn remove_cloud(&self, path: &str) -> Result<(), ArchiveDeletionError>;
    async fn cloud_exists(&self, path: &str) -> Result<bool, ArchiveDeletionError>;
}

pub struct OpenDalArchiveDeletionBackend {
    operator: Operator,
}

impl OpenDalArchiveDeletionBackend {
    pub fn new(operator: Operator) -> Self {
        Self { operator }
    }
}

#[async_trait]
impl ArchiveDeletionBackend for OpenDalArchiveDeletionBackend {
    async fn remove_local(&self, path: &Path) -> Result<(), ArchiveDeletionError> {
        match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    async fn remove_cloud(&self, path: &str) -> Result<(), ArchiveDeletionError> {
        self.operator.delete(path).await?;
        Ok(())
    }

    async fn cloud_exists(&self, path: &str) -> Result<bool, ArchiveDeletionError> {
        Ok(self.operator.exists(path).await?)
    }
}

pub struct GlobalSnapshotDeletion;

pub struct SnapshotDeletionRequest<'a> {
    pub game_id: &'a str,
    pub snapshot_id: &'a str,
    pub acting_device: &'a str,
    pub kind: DeletionKind,
    pub local_archive_path: &'a Path,
    pub cloud_archive_path: &'a str,
}

impl GlobalSnapshotDeletion {
    pub async fn execute<T: ManifestTransport, B: ArchiveDeletionBackend>(
        repository: &CloudManifestRepository<T>,
        backend: &B,
        request: SnapshotDeletionRequest<'_>,
    ) -> Result<(), GlobalSnapshotDeletionError> {
        if repository
            .load()
            .await?
            .games
            .get(request.game_id)
            .is_some_and(|game| game.is_final_tombstone(request.snapshot_id))
        {
            return Ok(());
        }

        repository
            .mutate(|manifest| {
                manifest.game_mut(request.game_id).begin_deletion(
                    request.snapshot_id,
                    request.acting_device,
                    request.kind.clone(),
                )?;
                Ok(())
            })
            .await?;

        backend.remove_local(request.local_archive_path).await?;
        repository
            .mutate(|manifest| {
                manifest
                    .game_mut(request.game_id)
                    .mark_acting_local_removed(request.snapshot_id, request.acting_device)
            })
            .await?;

        backend.remove_cloud(request.cloud_archive_path).await?;
        if backend.cloud_exists(request.cloud_archive_path).await? {
            return Err(GlobalSnapshotDeletionError::CloudArchiveStillPresent(
                request.cloud_archive_path.to_string(),
            ));
        }
        repository
            .mutate(|manifest| {
                manifest
                    .game_mut(request.game_id)
                    .mark_cloud_absent(request.snapshot_id, request.acting_device)
            })
            .await?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ArchiveDeletionError {
    #[error("Local Archive deletion error: {0}")]
    Local(#[from] std::io::Error),
    #[error("Cloud Archive deletion error: {0}")]
    Cloud(#[from] opendal::Error),
}

#[derive(Debug, Error)]
pub enum GlobalSnapshotDeletionError {
    #[error(transparent)]
    Repository(#[from] ManifestRepositoryError),
    #[error(transparent)]
    Archive(#[from] ArchiveDeletionError),
    #[error("Cloud Archive is still provider-visible after deletion: {0}")]
    CloudArchiveStillPresent(String),
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use opendal::services;

    use super::*;
    use crate::backup::CreatedBy;
    use crate::cloud_sync::v2::{
        ArchiveIntegrity, CloudManifest, GameManifest, ManifestTransport, SnapshotNode,
        SnapshotState,
    };

    struct MemoryManifestTransport {
        bytes: std::sync::Mutex<Option<Vec<u8>>>,
    }

    #[async_trait]
    impl ManifestTransport for MemoryManifestTransport {
        async fn read(&self) -> Result<Option<Vec<u8>>, opendal::Error> {
            Ok(self.bytes.lock().unwrap().clone())
        }

        async fn write(&self, bytes: &[u8]) -> Result<(), opendal::Error> {
            *self.bytes.lock().unwrap() = Some(bytes.to_vec());
            Ok(())
        }
    }

    struct FailLocalOnce<B> {
        inner: B,
        fail: AtomicBool,
    }

    struct ReportCloudPresentOnce<B> {
        inner: B,
        report_present: AtomicBool,
    }

    #[async_trait]
    impl<B: ArchiveDeletionBackend> ArchiveDeletionBackend for FailLocalOnce<B> {
        async fn remove_local(&self, path: &Path) -> Result<(), ArchiveDeletionError> {
            if self.fail.swap(false, Ordering::SeqCst) {
                return Err(std::io::Error::other("injected local failure").into());
            }
            self.inner.remove_local(path).await
        }

        async fn remove_cloud(&self, path: &str) -> Result<(), ArchiveDeletionError> {
            self.inner.remove_cloud(path).await
        }

        async fn cloud_exists(&self, path: &str) -> Result<bool, ArchiveDeletionError> {
            self.inner.cloud_exists(path).await
        }
    }

    #[async_trait]
    impl<B: ArchiveDeletionBackend> ArchiveDeletionBackend for ReportCloudPresentOnce<B> {
        async fn remove_local(&self, path: &Path) -> Result<(), ArchiveDeletionError> {
            self.inner.remove_local(path).await
        }

        async fn remove_cloud(&self, path: &str) -> Result<(), ArchiveDeletionError> {
            self.inner.remove_cloud(path).await
        }

        async fn cloud_exists(&self, path: &str) -> Result<bool, ArchiveDeletionError> {
            if self.report_present.swap(false, Ordering::SeqCst) {
                return Ok(true);
            }
            self.inner.cloud_exists(path).await
        }
    }

    async fn repository() -> CloudManifestRepository<MemoryManifestTransport> {
        let transport = MemoryManifestTransport {
            bytes: std::sync::Mutex::new(None),
        };
        let repository = CloudManifestRepository::with_transport(transport, 2);
        repository
            .mutate(|manifest| {
                let game = manifest
                    .games
                    .entry("game".into())
                    .or_insert_with(|| GameManifest::new("game"));
                game.upsert_live(SnapshotNode::live(
                    "snapshot",
                    None,
                    ArchiveIntegrity {
                        size: 1,
                        xxh3_64: "0000000000000000".into(),
                    },
                    CreatedBy::Manual,
                ))?;
                game.set_head("pc".into(), "snapshot".into());
                game.report_local_archive("pc".into(), "snapshot".into(), true);
                Ok(())
            })
            .await
            .unwrap();
        repository
    }

    fn memory_operator() -> Operator {
        Operator::new(services::Memory::default()).unwrap().finish()
    }

    #[tokio::test]
    async fn deletion_is_pending_before_bytes_and_retry_reaches_final() {
        let repository = repository().await;
        let operator = memory_operator();
        operator.write("archive.7z", b"x".to_vec()).await.unwrap();
        let root = temp_dir::TempDir::new().unwrap();
        let local = root.path().join("archive.7z");
        tokio::fs::write(&local, b"x").await.unwrap();
        let backend = FailLocalOnce {
            inner: OpenDalArchiveDeletionBackend::new(operator.clone()),
            fail: AtomicBool::new(true),
        };

        assert!(
            GlobalSnapshotDeletion::execute(
                &repository,
                &backend,
                SnapshotDeletionRequest {
                    game_id: "game",
                    snapshot_id: "snapshot",
                    acting_device: "pc",
                    kind: DeletionKind::User,
                    local_archive_path: &local,
                    cloud_archive_path: "archive.7z",
                },
            )
            .await
            .is_err()
        );
        let pending = repository.load().await.unwrap();
        assert!(matches!(
            pending.games["game"].snapshots["snapshot"].state,
            SnapshotState::PendingTombstone(_)
        ));
        assert!(local.exists());
        assert!(operator.exists("archive.7z").await.unwrap());

        GlobalSnapshotDeletion::execute(
            &repository,
            &backend,
            SnapshotDeletionRequest {
                game_id: "game",
                snapshot_id: "snapshot",
                acting_device: "pc",
                kind: DeletionKind::User,
                local_archive_path: &local,
                cloud_archive_path: "archive.7z",
            },
        )
        .await
        .unwrap();

        let final_manifest: CloudManifest = repository.load().await.unwrap();
        assert!(final_manifest.games["game"].is_final_tombstone("snapshot"));
        assert!(!local.exists());
        assert!(!operator.exists("archive.7z").await.unwrap());
    }

    #[tokio::test]
    async fn provider_visibility_failure_remains_pending_and_is_retryable() {
        let repository = repository().await;
        let operator = memory_operator();
        operator.write("archive.7z", b"x".to_vec()).await.unwrap();
        let root = temp_dir::TempDir::new().unwrap();
        let local = root.path().join("archive.7z");
        tokio::fs::write(&local, b"x").await.unwrap();
        let backend = ReportCloudPresentOnce {
            inner: OpenDalArchiveDeletionBackend::new(operator),
            report_present: AtomicBool::new(true),
        };
        let request = || SnapshotDeletionRequest {
            game_id: "game",
            snapshot_id: "snapshot",
            acting_device: "pc",
            kind: DeletionKind::User,
            local_archive_path: &local,
            cloud_archive_path: "archive.7z",
        };

        assert!(matches!(
            GlobalSnapshotDeletion::execute(&repository, &backend, request()).await,
            Err(GlobalSnapshotDeletionError::CloudArchiveStillPresent(_))
        ));
        assert!(matches!(
            repository.load().await.unwrap().games["game"].snapshots["snapshot"].state,
            SnapshotState::PendingTombstone(_)
        ));

        GlobalSnapshotDeletion::execute(&repository, &backend, request())
            .await
            .unwrap();
        assert!(repository.load().await.unwrap().games["game"].is_final_tombstone("snapshot"));
    }
}
