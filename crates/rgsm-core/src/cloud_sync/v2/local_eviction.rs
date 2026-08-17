use std::path::PathBuf;

use opendal::Operator;
use thiserror::Error;

use super::{
    CLOUD_MANIFEST_PATH, CloudManifestRepository, ManifestError, ManifestRepositoryError,
    SnapshotState, cloud_archive_path,
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

pub struct CloudArchiveEviction {
    operator: Operator,
    max_attempts: usize,
}

impl CloudArchiveEviction {
    pub fn new(operator: Operator, max_attempts: usize) -> Self {
        Self {
            operator,
            max_attempts: max_attempts.max(1),
        }
    }

    /// Remove only the Cloud Archive bytes. Shared Snapshot metadata, Heads,
    /// and Device Archive reports stay in the catalog.
    pub async fn evict(
        &self,
        game_id: &str,
        snapshot_id: &str,
    ) -> Result<bool, CloudArchiveEvictionError> {
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
        let SnapshotState::Live(live) = &node.state else {
            return Err(ManifestError::ExpectedLive(snapshot_id.to_string()).into());
        };
        let existed = live.cloud_archive_verified;
        let remote = cloud_archive_path(game_id, snapshot_id, node.archive_format)
            .map_err(|error| CloudArchiveEvictionError::RemotePath(error.to_string()))?;

        // Durably mark the eviction in the manifest BEFORE deleting bytes.
        // If the delete fails after this, the metadata correctly says
        // unavailable and automatic upload is suppressed. Orphaned bytes
        // in the cloud are harmless; the reverse is not.
        let game_id_owned = game_id.to_string();
        let snapshot_id_owned = snapshot_id.to_string();
        repository
            .mutate(move |manifest| {
                let game = manifest
                    .games
                    .get_mut(&game_id_owned)
                    .ok_or_else(|| ManifestError::MissingGame(game_id_owned.clone()))?;
                let node = game
                    .snapshots
                    .get_mut(&snapshot_id_owned)
                    .ok_or_else(|| ManifestError::MissingSnapshot(snapshot_id_owned.clone()))?;
                let SnapshotState::Live(live) = &mut node.state else {
                    return Err(ManifestError::ExpectedLive(snapshot_id_owned.clone()));
                };
                live.cloud_archive_verified = false;
                live.cloud_evicted = true;
                Ok(())
            })
            .await?;

        match self.operator.delete(&remote).await {
            Ok(()) => {}
            Err(error) if error.kind() == opendal::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(existed)
    }
}

#[derive(Debug, Error)]
pub enum CloudArchiveEvictionError {
    #[error(transparent)]
    Repository(#[from] ManifestRepositoryError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("Cloud Archive path is invalid: {0}")]
    RemotePath(String),
    #[error(transparent)]
    Operator(#[from] opendal::Error),
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
