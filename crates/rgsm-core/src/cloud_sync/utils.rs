use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use log::info;
use opendal::{ErrorKind, Operator};
use thiserror::Error;
use tokio::fs;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::backup::{Game, GameSnapshots, Snapshot, archive_file_name, remote_archive_path};
use crate::cloud_sync::transfer::{CloudTransfer, path_to_remote_key};
use crate::cloud_sync::{V1_CONFIG_PATH, V1_SAVE_DATA_PREFIX};
use crate::config::{
    Config, get_backup_path, get_config, replace_config_local, resolve_backup_path,
};
use crate::preclude::*;

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

fn acquire_error() -> BackendError {
    BackendError::Unexpected(anyhow::anyhow!("Semaphore closed unexpectedly"))
}

fn join_error(e: tokio::task::JoinError) -> BackendError {
    BackendError::Unexpected(anyhow::anyhow!("Task panicked: {e}"))
}

fn stage_dir_name(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!(".{prefix}.{millis}")
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

pub async fn load_remote_config(
    op: &Operator,
    token: Option<&CancellationToken>,
) -> Result<Config, SyncOperationError> {
    let transfer = CloudTransfer::new(op);
    let config_bytes =
        run_with_optional_cancel(token, transfer.download_bytes_streaming(V1_CONFIG_PATH)).await?;
    serde_json::from_slice(&config_bytes)
        .map_err(BackendError::from)
        .map_err(Into::into)
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

pub async fn upload_game_snapshots(
    op: &Operator,
    storage_key: &str,
    info: &GameSnapshots,
) -> Result<(), BackendError> {
    let transfer = CloudTransfer::new(op);
    let metadata_path = game_cloud_metadata_path(storage_key)?;
    transfer
        .upload_bytes_streaming(&serde_json::to_vec_pretty(info)?, &metadata_path)
        .await?;
    Ok(())
}

pub async fn upload_config(op: &Operator) -> Result<(), BackendError> {
    let config = get_config()?;
    upload_config_snapshot(op, &config).await
}

pub async fn upload_config_snapshot(op: &Operator, config: &Config) -> Result<(), BackendError> {
    let transfer = CloudTransfer::new(op);
    transfer
        .upload_bytes_streaming(&serde_json::to_vec_pretty(config)?, V1_CONFIG_PATH)
        .await?;
    Ok(())
}

pub async fn upload_game_data(
    op: &Operator,
    game: &Game,
    max_concurrency: usize,
    token: Option<CancellationToken>,
) -> Result<GameSnapshots, SyncOperationError> {
    let dir_name = game.backup_dir_name().into_owned();
    let backup_info = game
        .get_game_snapshots_info()
        .map_err(SyncOperationError::from)?;
    let backup_root = get_backup_path().map_err(SyncOperationError::from)?;

    let metadata_path = game_cloud_metadata_path(&dir_name).map_err(SyncOperationError::Backend)?;
    let transfer = CloudTransfer::new(op);
    run_with_optional_cancel(
        token.as_ref(),
        transfer.upload_bytes_streaming(
            &serde_json::to_vec_pretty(&backup_info).map_err(BackendError::from)?,
            &metadata_path,
        ),
    )
    .await?;

    let semaphore = Arc::new(Semaphore::new(max_concurrency.max(1)));
    let mut tasks = JoinSet::new();

    for backup in &backup_info.backups {
        let cloud_archive_path =
            game_cloud_archive_path(&dir_name, backup).map_err(SyncOperationError::Backend)?;
        let local_archive_path = if backup.path.is_empty() {
            backup_root
                .join(&dir_name)
                .join(archive_file_name(&backup.date, backup.archive_format))
        } else {
            PathBuf::from(&backup.path)
        };
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| SyncOperationError::Backend(acquire_error()))?;
        let op_clone = op.clone();
        let child_token = token.as_ref().map(CancellationToken::child_token);
        tasks.spawn(async move {
            info!(
                target: "rgsm::cloud::utils",
                "Uploading {} from {}",
                cloud_archive_path,
                local_archive_path.display()
            );
            let transfer = CloudTransfer::new(&op_clone);
            let result = run_with_optional_cancel(
                child_token.as_ref(),
                transfer.upload_file_streaming(&local_archive_path, &cloud_archive_path),
            )
            .await;
            drop(permit);
            result
        });
    }

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(err),
            Err(err) => return Err(SyncOperationError::Backend(join_error(err))),
        }
    }

    Ok(backup_info)
}

pub async fn stage_remote_game_download(
    op: &Operator,
    storage_key: &str,
    stage_root: &Path,
    max_concurrency: usize,
    token: Option<CancellationToken>,
) -> Result<GameSnapshots, SyncOperationError> {
    let backup_info = load_remote_game_snapshots(op, storage_key, token.as_ref())
        .await?
        .ok_or_else(|| {
            BackendError::Unexpected(anyhow::anyhow!(
                "Remote metadata for '{}' not found",
                storage_key
            ))
        })
        .map_err(SyncOperationError::Backend)?;

    let stage_game_dir = stage_root.join(storage_key);
    fs::create_dir_all(&stage_game_dir)
        .await
        .map_err(BackendError::from)
        .map_err(SyncOperationError::Backend)?;

    let semaphore = Arc::new(Semaphore::new(max_concurrency.max(1)));
    let mut tasks = JoinSet::new();

    for backup in &backup_info.backups {
        let cloud_archive_path =
            game_cloud_archive_path(storage_key, backup).map_err(SyncOperationError::Backend)?;
        let local_archive_path =
            stage_game_dir.join(archive_file_name(&backup.date, backup.archive_format));
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| SyncOperationError::Backend(acquire_error()))?;
        let op_clone = op.clone();
        let child_token = token.as_ref().map(CancellationToken::child_token);
        tasks.spawn(async move {
            info!(
                target: "rgsm::cloud::utils",
                "Downloading {} to {}",
                cloud_archive_path,
                local_archive_path.display()
            );
            let transfer = CloudTransfer::new(&op_clone);
            let result = run_with_optional_cancel(
                child_token.as_ref(),
                transfer.download_file_streaming(&cloud_archive_path, &local_archive_path),
            )
            .await;
            drop(permit);
            result
        });
    }

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(err),
            Err(err) => return Err(SyncOperationError::Backend(join_error(err))),
        }
    }

    let mut updated_info = backup_info.clone();
    for backup in &mut updated_info.backups {
        let local_archive_path =
            stage_game_dir.join(archive_file_name(&backup.date, backup.archive_format));
        backup.path = local_archive_path.to_string_lossy().to_string();
    }

    let transfer = CloudTransfer::new(op);
    run_with_optional_cancel(
        token.as_ref(),
        transfer.write_local_bytes_atomically(
            &stage_game_dir.join("Backups.json"),
            &serde_json::to_vec_pretty(&updated_info).map_err(BackendError::from)?,
        ),
    )
    .await?;

    Ok(updated_info)
}

async fn replace_directory(stage_dir: &Path, final_dir: &Path) -> Result<(), BackendError> {
    if let Some(parent) = final_dir.parent() {
        fs::create_dir_all(parent).await?;
    }

    let rollback_dir = final_dir.with_file_name(stage_dir_name("game-rollback"));
    if rollback_dir.exists() {
        fs::remove_dir_all(&rollback_dir).await?;
    }

    if final_dir.exists() {
        fs::rename(final_dir, &rollback_dir).await?;
    }

    if let Err(err) = fs::rename(stage_dir, final_dir).await {
        if rollback_dir.exists() {
            let _ = fs::rename(&rollback_dir, final_dir).await;
        }
        return Err(err.into());
    }

    if rollback_dir.exists() {
        fs::remove_dir_all(&rollback_dir).await?;
    }
    Ok(())
}

pub async fn replace_local_game_from_stage(
    stage_root: &Path,
    backup_root: &Path,
    storage_key: &str,
) -> Result<(), BackendError> {
    let stage_game_dir = stage_root.join(storage_key);
    let local_game_dir = backup_root.join(storage_key);
    replace_directory(&stage_game_dir, &local_game_dir).await
}

fn final_snapshot_path(backup_root: &Path, storage_key: &str, snapshot: &Snapshot) -> String {
    backup_root
        .join(storage_key)
        .join(archive_file_name(&snapshot.date, snapshot.archive_format))
        .to_string_lossy()
        .to_string()
}

async fn rewrite_staged_snapshot_paths(
    stage_root: &Path,
    backup_root: &Path,
    storage_key: &str,
    info: &mut GameSnapshots,
) -> Result<(), BackendError> {
    for snapshot in &mut info.backups {
        snapshot.path = final_snapshot_path(backup_root, storage_key, snapshot);
    }

    let metadata_path = stage_root.join(storage_key).join("Backups.json");
    fs::write(metadata_path, serde_json::to_vec_pretty(info)?).await?;
    Ok(())
}

pub async fn replace_local_game_with_remote(
    session: &crate::cloud_sync::CloudSyncSessionConfig,
    game: &Game,
    op: &Operator,
    stage_prefix: &str,
    token: Option<CancellationToken>,
) -> Result<GameSnapshots, SyncOperationError> {
    let backup_root = get_backup_path().map_err(SyncOperationError::from)?;
    let storage_key = game.backup_dir_name().into_owned();
    let stage_root = new_stage_root(&backup_root, stage_prefix);
    if stage_root.exists() {
        let _ = fs::remove_dir_all(&stage_root).await;
    }
    fs::create_dir_all(&stage_root)
        .await
        .map_err(BackendError::from)
        .map_err(SyncOperationError::Backend)?;

    let result = async {
        let mut downloaded = stage_remote_game_download(
            op,
            &storage_key,
            &stage_root,
            session.normalized_max_concurrency(),
            token,
        )
        .await?;
        rewrite_staged_snapshot_paths(&stage_root, &backup_root, &storage_key, &mut downloaded)
            .await
            .map_err(SyncOperationError::Backend)?;
        replace_local_game_from_stage(&stage_root, &backup_root, &storage_key)
            .await
            .map_err(SyncOperationError::Backend)?;
        Ok(downloaded)
    }
    .await;

    if stage_root.exists() {
        let _ = fs::remove_dir_all(&stage_root).await;
    }

    result
}

pub async fn commit_staged_backup_root(
    stage_root: &Path,
    target_backup_root: &Path,
    remote_config: &Config,
) -> Result<(), BackendError> {
    let rollback_root = target_backup_root.with_file_name(stage_dir_name("download-rollback"));
    if rollback_root.exists() {
        fs::remove_dir_all(&rollback_root).await?;
    }

    if target_backup_root.exists() {
        fs::rename(target_backup_root, &rollback_root).await?;
    }

    if let Err(err) = fs::rename(stage_root, target_backup_root).await {
        if rollback_root.exists() {
            let _ = fs::rename(&rollback_root, target_backup_root).await;
        }
        return Err(err.into());
    }

    if let Err(err) = replace_config_local(remote_config) {
        let _ = fs::remove_dir_all(target_backup_root).await;
        if rollback_root.exists() {
            let _ = fs::rename(&rollback_root, target_backup_root).await;
        }
        return Err(BackendError::from(err));
    }

    if rollback_root.exists() {
        fs::remove_dir_all(&rollback_root).await?;
    }
    Ok(())
}

pub fn target_backup_root_from_config(config: &Config) -> PathBuf {
    resolve_backup_path(&config.backup_path)
}

pub fn new_stage_root(target_backup_root: &Path, prefix: &str) -> PathBuf {
    target_backup_root.with_file_name(stage_dir_name(prefix))
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
    fn cloud_and_final_paths_follow_snapshot_archive_format() {
        let v4 = snapshot(ArchiveFormat::SevenZ);
        assert_eq!(
            game_cloud_archive_path("test-game", &v4).unwrap(),
            "save_data/test-game/2026-07-13_12-00-00.7z"
        );
        assert!(
            final_snapshot_path(Path::new("backup"), "test-game", &v4).ends_with(
                Path::new("test-game")
                    .join("2026-07-13_12-00-00.7z")
                    .to_string_lossy()
                    .as_ref()
            )
        );

        let legacy = snapshot(ArchiveFormat::Zip);
        assert_eq!(
            game_cloud_archive_path("test-game", &legacy).unwrap(),
            "save_data/test-game/2026-07-13_12-00-00.zip"
        );
    }
}
