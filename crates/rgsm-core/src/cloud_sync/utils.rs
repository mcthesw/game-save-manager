use crate::backup::{GameSnapshots, Snapshot, remote_archive_path};
use crate::cloud_sync::V1_SAVE_DATA_PREFIX;
use crate::cloud_sync::transfer::{CloudTransfer, path_to_remote_key};
use crate::preclude::*;
use opendal::{ErrorKind, Operator};
use std::future::Future;
use std::path::PathBuf;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Error)]
pub enum SyncOperationError {
    #[error("cloud sync operation cancelled")]
    Cancelled,
    #[error(transparent)]
    Backend(#[from] BackendError),
}

impl From<ConfigError> for SyncOperationError {
    fn from(value: ConfigError) -> Self {
        Self::Backend(BackendError::from(value))
    }
}

impl From<BackupError> for SyncOperationError {
    fn from(value: BackupError) -> Self {
        Self::Backend(BackendError::from(value))
    }
}

async fn run_with_optional_cancel<T, F>(
    token: Option<&CancellationToken>,
    future: F,
) -> Result<T, SyncOperationError>
where
    F: Future<Output = Result<T, BackendError>>,
{
    if let Some(token) = token {
        tokio::select! {
            _ = token.cancelled() => Err(SyncOperationError::Cancelled),
            result = future => result.map_err(SyncOperationError::Backend),
        }
    } else {
        future.await.map_err(SyncOperationError::Backend)
    }
}

pub fn is_not_found(err: &BackendError) -> bool {
    matches!(err, BackendError::Cloud(inner) if inner.kind() == ErrorKind::NotFound)
}

pub fn game_cloud_metadata_path(storage_key: &str) -> Result<String, BackendError> {
    let backups_json = PathBuf::from(V1_SAVE_DATA_PREFIX)
        .join(storage_key)
        .join("Backups.json");
    path_to_remote_key(&backups_json)
}

pub fn game_cloud_archive_path(
    storage_key: &str,
    snapshot: &Snapshot,
) -> Result<String, BackendError> {
    path_to_remote_key(&remote_archive_path(
        storage_key,
        &snapshot.date,
        snapshot.archive_format,
    ))
}

pub async fn load_remote_game_snapshots(
    op: &Operator,
    storage_key: &str,
    token: Option<&CancellationToken>,
) -> Result<Option<GameSnapshots>, SyncOperationError> {
    let transfer = CloudTransfer::new(op);
    let metadata_path =
        game_cloud_metadata_path(storage_key).map_err(SyncOperationError::Backend)?;
    match run_with_optional_cancel(token, transfer.download_bytes_streaming(&metadata_path)).await {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(BackendError::from)
            .map_err(Into::into),
        Err(SyncOperationError::Backend(err)) if is_not_found(&err) => Ok(None),
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{ArchiveFormat, CreatedBy};

    fn snapshot(format: ArchiveFormat) -> Snapshot {
        Snapshot {
            date: "2026-07-13_12-00-00".into(),
            describe: String::new(),
            path: String::new(),
            archive_format: format,
            size: 0,
            parent: None,
            archive_hash: None,
            created_at: None,
            device_id: None,
            created_by: CreatedBy::Manual,
        }
    }

    #[test]
    fn legacy_archive_paths_follow_snapshot_archive_format() {
        let v4 = snapshot(ArchiveFormat::SevenZ);
        assert_eq!(
            game_cloud_archive_path("test-game", &v4).unwrap(),
            "save_data/test-game/2026-07-13_12-00-00.7z"
        );

        let legacy = snapshot(ArchiveFormat::Zip);
        assert_eq!(
            game_cloud_archive_path("test-game", &legacy).unwrap(),
            "save_data/test-game/2026-07-13_12-00-00.zip"
        );
    }
}
