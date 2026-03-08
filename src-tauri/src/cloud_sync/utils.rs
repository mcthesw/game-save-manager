use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use log::info;
use opendal::{ErrorKind, Operator};
use tokio::fs;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::backup::GameSnapshots;
use crate::cloud_sync::transfer::{CloudTransfer, path_to_remote_key};
use crate::config::{Config, get_backup_path, get_config, resolve_backup_path, set_config_local};
use crate::preclude::*;

#[derive(Debug)]
pub enum SyncOperationError {
    Cancelled,
    Backend(BackendError),
}

impl From<BackendError> for SyncOperationError {
    fn from(value: BackendError) -> Self {
        Self::Backend(value)
    }
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

pub fn game_cloud_metadata_path(game_name: &str) -> Result<String, BackendError> {
    let backups_json = PathBuf::from("save_data")
        .join(game_name)
        .join("Backups.json");
    path_to_remote_key(&backups_json)
}

pub fn game_cloud_zip_path(game_name: &str, backup_date: &str) -> Result<String, BackendError> {
    let zip_path = PathBuf::from("save_data")
        .join(game_name)
        .join(format!("{backup_date}.zip"));
    path_to_remote_key(&zip_path)
}

pub async fn load_remote_config(
    op: &Operator,
    token: Option<&CancellationToken>,
) -> Result<Config, SyncOperationError> {
    let transfer = CloudTransfer::new(op);
    let config_bytes = run_with_optional_cancel(
        token,
        transfer.download_bytes_streaming("/GameSaveManager.config.json"),
    )
    .await?;
    serde_json::from_slice(&config_bytes)
        .map_err(BackendError::from)
        .map_err(Into::into)
}

pub async fn load_remote_game_snapshots(
    op: &Operator,
    game_name: &str,
    token: Option<&CancellationToken>,
) -> Result<Option<GameSnapshots>, SyncOperationError> {
    let transfer = CloudTransfer::new(op);
    let metadata_path = game_cloud_metadata_path(game_name).map_err(SyncOperationError::Backend)?;
    match run_with_optional_cancel(token, transfer.download_bytes_streaming(&metadata_path)).await {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(BackendError::from)
            .map_err(Into::into),
        Err(SyncOperationError::Backend(err)) if is_not_found(&err) => Ok(None),
        Err(err) => Err(err),
    }
}

pub async fn upload_game_snapshots(op: &Operator, info: GameSnapshots) -> Result<(), BackendError> {
    let transfer = CloudTransfer::new(op);
    let metadata_path = game_cloud_metadata_path(&info.name)?;
    transfer
        .upload_bytes_streaming(&serde_json::to_vec_pretty(&info)?, &metadata_path)
        .await?;
    Ok(())
}

pub async fn upload_config(op: &Operator) -> Result<(), BackendError> {
    let transfer = CloudTransfer::new(op);
    let config = get_config()?;
    transfer
        .upload_bytes_streaming(
            &serde_json::to_vec_pretty(&config)?,
            "/GameSaveManager.config.json",
        )
        .await?;
    Ok(())
}

pub async fn upload_game_data(
    op: &Operator,
    game_name: &str,
    max_concurrency: usize,
    token: Option<CancellationToken>,
) -> Result<GameSnapshots, SyncOperationError> {
    let config = get_config().map_err(SyncOperationError::from)?;
    let game = config
        .games
        .iter()
        .find(|g| g.name == game_name)
        .ok_or_else(|| BackendError::Unexpected(anyhow::anyhow!("Game '{}' not found", game_name)))
        .map_err(SyncOperationError::Backend)?;

    let backup_info = game
        .get_game_snapshots_info()
        .map_err(SyncOperationError::from)?;
    let backup_root = get_backup_path().map_err(SyncOperationError::from)?;

    let metadata_path = game_cloud_metadata_path(game_name).map_err(SyncOperationError::Backend)?;
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
        let cloud_zip_path =
            game_cloud_zip_path(game_name, &backup.date).map_err(SyncOperationError::Backend)?;
        let local_zip_path = if backup.path.is_empty() {
            backup_root
                .join(game_name)
                .join(format!("{}.zip", backup.date))
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
                cloud_zip_path,
                local_zip_path.display()
            );
            let transfer = CloudTransfer::new(&op_clone);
            let result = run_with_optional_cancel(
                child_token.as_ref(),
                transfer.upload_file_streaming(&local_zip_path, &cloud_zip_path),
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
    game_name: &str,
    stage_root: &Path,
    max_concurrency: usize,
    token: Option<CancellationToken>,
) -> Result<GameSnapshots, SyncOperationError> {
    let backup_info = load_remote_game_snapshots(op, game_name, token.as_ref())
        .await?
        .ok_or_else(|| {
            BackendError::Unexpected(anyhow::anyhow!(
                "Remote metadata for '{}' not found",
                game_name
            ))
        })
        .map_err(SyncOperationError::Backend)?;

    let stage_game_dir = stage_root.join(game_name);
    fs::create_dir_all(&stage_game_dir)
        .await
        .map_err(BackendError::from)
        .map_err(SyncOperationError::Backend)?;

    let semaphore = Arc::new(Semaphore::new(max_concurrency.max(1)));
    let mut tasks = JoinSet::new();

    for backup in &backup_info.backups {
        let cloud_zip_path =
            game_cloud_zip_path(game_name, &backup.date).map_err(SyncOperationError::Backend)?;
        let local_zip_path = stage_game_dir.join(format!("{}.zip", backup.date));
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
                cloud_zip_path,
                local_zip_path.display()
            );
            let transfer = CloudTransfer::new(&op_clone);
            let result = run_with_optional_cancel(
                child_token.as_ref(),
                transfer.download_file_streaming(&cloud_zip_path, &local_zip_path),
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
        let local_zip_path = stage_game_dir.join(format!("{}.zip", backup.date));
        backup.path = local_zip_path.to_string_lossy().to_string();
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
    if final_dir.exists() {
        fs::remove_dir_all(final_dir).await?;
    }
    fs::rename(stage_dir, final_dir).await?;
    Ok(())
}

pub async fn replace_local_game_from_stage(
    stage_root: &Path,
    backup_root: &Path,
    game_name: &str,
) -> Result<(), BackendError> {
    let stage_game_dir = stage_root.join(game_name);
    let local_game_dir = backup_root.join(game_name);
    replace_directory(&stage_game_dir, &local_game_dir).await
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

    if let Err(err) = set_config_local(remote_config) {
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
