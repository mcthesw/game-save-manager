use std::path::PathBuf;

use opendal::Operator;
use thiserror::Error;

use super::{
    CLOUD_MANIFEST_PATH, CloudManifestRepository, ManifestError, ManifestRepositoryError,
    SnapshotState,
};
use crate::backup::archive_path;
use crate::device::DeviceId;

pub struct LocalArchiveEviction {
    operator: Operator,
    local_archive_root: PathBuf,
    current_device_id: DeviceId,
    max_attempts: usize,
}

impl LocalArchiveEviction {
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

    /// Remove only this Device's Archive copy and availability report. Shared
    /// Snapshot metadata, Heads, and Cloud Archive availability are untouched.
    pub async fn evict(
        &self,
        game_id: &str,
        snapshot_id: &str,
    ) -> Result<bool, LocalArchiveEvictionError> {
        let repository = CloudManifestRepository::new(
            self.operator.clone(),
            CLOUD_MANIFEST_PATH,
            self.max_attempts,
        );
        let manifest = repository.load().await?;
        let game = manifest
            .games
            .get(game_id)
            .ok_or_else(|| ManifestError::MissingGame(game_id.to_string()))?;
        let node = game
            .snapshots
            .get(snapshot_id)
            .ok_or_else(|| ManifestError::MissingSnapshot(snapshot_id.to_string()))?;
        if !matches!(node.state, SnapshotState::Live(_)) {
            return Err(ManifestError::ExpectedLive(snapshot_id.to_string()).into());
        }
        let local_path = archive_path(
            &self.local_archive_root.join(game_id),
            snapshot_id,
            node.archive_format,
        );
        let existed = local_path.exists();
        remove_file_if_exists(&local_path).await?;

        let game_id = game_id.to_string();
        let snapshot_id = snapshot_id.to_string();
        let current_device = self.current_device_id.clone();
        repository
            .mutate(move |manifest| {
                let game = manifest
                    .games
                    .get_mut(&game_id)
                    .ok_or_else(|| ManifestError::MissingGame(game_id.clone()))?;
                game.report_local_archive(current_device.clone(), snapshot_id.clone(), false);
                Ok(())
            })
            .await?;
        Ok(existed)
    }
}

async fn remove_file_if_exists(path: &std::path::Path) -> Result<(), std::io::Error> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[derive(Debug, Error)]
pub enum LocalArchiveEvictionError {
    #[error(transparent)]
    Repository(#[from] ManifestRepositoryError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("Local Archive eviction failed: {0}")]
    Io(#[from] std::io::Error),
}
