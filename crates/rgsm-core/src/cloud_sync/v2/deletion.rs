use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use opendal::Operator;
use thiserror::Error;

use super::{
    CLOUD_MANIFEST_PATH, CloudManifestRepository, DeletionKind, ManifestRepositoryError,
    ManifestTransport, SnapshotState, cloud_archive_path,
};
use crate::backup::{ArchiveFormat, archive_path};
use crate::device::DeviceId;
use crate::preclude::BackendError;

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

pub struct SnapshotDeletionLifecycle {
    operator: Operator,
    local_archive_root: PathBuf,
    current_device_id: DeviceId,
    max_attempts: usize,
}

impl SnapshotDeletionLifecycle {
    pub fn new(
        operator: Operator,
        local_archive_root: PathBuf,
        current_device_id: DeviceId,
        max_attempts: usize,
    ) -> Self {
        Self {
            operator,
            local_archive_root,
            current_device_id,
            max_attempts: max_attempts.max(1),
        }
    }

    pub async fn delete_snapshot(
        &self,
        game_id: &str,
        snapshot_id: &str,
        confirmed: bool,
    ) -> Result<(), SnapshotDeletionLifecycleError> {
        self.delete_snapshot_with_kind(
            game_id,
            snapshot_id,
            confirmed.then_some(DeletionKind::User),
        )
        .await
    }

    pub(crate) async fn delete_retention_snapshot(
        &self,
        game_id: &str,
        snapshot_id: &str,
    ) -> Result<(), SnapshotDeletionLifecycleError> {
        self.delete_snapshot_with_kind(game_id, snapshot_id, Some(DeletionKind::Retention))
            .await
    }

    async fn delete_snapshot_with_kind(
        &self,
        game_id: &str,
        snapshot_id: &str,
        requested_kind: Option<DeletionKind>,
    ) -> Result<(), SnapshotDeletionLifecycleError> {
        let repository = self.repository();
        let manifest = repository.load().await?;
        let game = manifest
            .games
            .get(game_id)
            .ok_or_else(|| SnapshotDeletionLifecycleError::GameNotFound(game_id.to_string()))?;
        let node = game.snapshots.get(snapshot_id).ok_or_else(|| {
            SnapshotDeletionLifecycleError::SnapshotNotFound(snapshot_id.to_string())
        })?;
        let kind = match &node.state {
            SnapshotState::Live(_) if requested_kind.is_some() => requested_kind
                .clone()
                .expect("checked requested deletion kind"),
            SnapshotState::Live(_) => {
                return Err(SnapshotDeletionLifecycleError::ConfirmationRequired);
            }
            SnapshotState::PendingTombstone(pending)
                if pending.acting_device == self.current_device_id
                    && requested_kind
                        .as_ref()
                        .is_none_or(|kind| kind == &pending.kind) =>
            {
                pending.kind.clone()
            }
            SnapshotState::PendingTombstone(pending) => {
                return Err(SnapshotDeletionLifecycleError::DeletionOwnedByDevice(
                    pending.acting_device.clone(),
                ));
            }
            SnapshotState::FinalTombstone { .. } => return Ok(()),
        };
        let local_path = self.local_path(game_id, snapshot_id, node.archive_format);
        let remote_path = cloud_archive_path(game_id, snapshot_id, node.archive_format)?;
        GlobalSnapshotDeletion::execute(
            &repository,
            &OpenDalArchiveDeletionBackend::new(self.operator.clone()),
            SnapshotDeletionRequest {
                game_id,
                snapshot_id,
                acting_device: &self.current_device_id,
                kind,
                local_archive_path: &local_path,
                cloud_archive_path: &remote_path,
            },
        )
        .await?;
        Ok(())
    }

    /// Deletes local Archive copies for every durable Tombstone before another
    /// transfer is planned. The returned IDs let the application remove legacy
    /// local presentation without reparenting the shared graph.
    pub async fn converge_local_tombstones(
        &self,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, SnapshotDeletionLifecycleError> {
        self.converge_local_tombstones_except(&BTreeSet::new())
            .await
    }

    pub(crate) async fn converge_local_tombstones_except(
        &self,
        excluded_games: &BTreeSet<String>,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, SnapshotDeletionLifecycleError> {
        let repository = self.repository();
        let manifest = repository.load().await?;
        let mut tombstones = BTreeMap::<String, BTreeSet<String>>::new();
        for (game_id, game) in &manifest.games {
            if excluded_games.contains(game_id) {
                continue;
            }
            for node in game.snapshots.values().filter(|node| !node.state.is_live()) {
                remove_file_if_exists(&self.local_path(
                    game_id,
                    &node.snapshot_id,
                    node.archive_format,
                ))
                .await?;
                tombstones
                    .entry(game_id.clone())
                    .or_default()
                    .insert(node.snapshot_id.clone());
            }
        }
        let has_stale_report = tombstones.iter().any(|(game_id, snapshot_ids)| {
            manifest
                .games
                .get(game_id)
                .and_then(|game| game.local_archives.get(&self.current_device_id))
                .is_some_and(|reported| !reported.is_disjoint(snapshot_ids))
        });
        if has_stale_report {
            let current_device = self.current_device_id.clone();
            let removed = tombstones.clone();
            repository
                .mutate(move |manifest| {
                    for (game_id, snapshot_ids) in &removed {
                        let Some(game) = manifest.games.get_mut(game_id) else {
                            continue;
                        };
                        for snapshot_id in snapshot_ids {
                            game.report_local_archive(
                                current_device.clone(),
                                snapshot_id.clone(),
                                false,
                            );
                        }
                    }
                    Ok(())
                })
                .await?;
        }
        Ok(tombstones)
    }

    fn repository(&self) -> CloudManifestRepository<super::OpenDalManifestTransport> {
        CloudManifestRepository::new(
            self.operator.clone(),
            CLOUD_MANIFEST_PATH,
            self.max_attempts,
        )
    }

    fn local_path(&self, game_id: &str, snapshot_id: &str, format: ArchiveFormat) -> PathBuf {
        archive_path(&self.local_archive_root.join(game_id), snapshot_id, format)
    }
}

async fn remove_file_if_exists(path: &Path) -> Result<(), std::io::Error> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
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
                let game = manifest.game_mut(request.game_id);
                if request.kind == DeletionKind::Retention {
                    game.ensure_retention_deletable(request.snapshot_id)?;
                }
                game.begin_deletion(
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

#[derive(Debug, Error)]
pub enum SnapshotDeletionLifecycleError {
    #[error("V2 Game not found: {0}")]
    GameNotFound(String),
    #[error("V2 Snapshot not found: {0}")]
    SnapshotNotFound(String),
    #[error("Permanent Snapshot deletion requires explicit confirmation")]
    ConfirmationRequired,
    #[error("This device's Current Position still points at {0}")]
    CurrentPositionBlocksDeletion(String),
    #[error("Snapshot deletion must be retried on the initiating Device: {0}")]
    DeletionOwnedByDevice(DeviceId),
    #[error(transparent)]
    Repository(#[from] ManifestRepositoryError),
    #[error(transparent)]
    Deletion(#[from] GlobalSnapshotDeletionError),
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error("Local Tombstone convergence failed: {0}")]
    Local(#[from] std::io::Error),
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
