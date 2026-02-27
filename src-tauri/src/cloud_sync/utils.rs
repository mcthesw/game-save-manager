use std::path::PathBuf;
use std::sync::Arc;

use log::info;
use opendal::Operator;
use tokio::fs;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::backup::GameSnapshots;
use crate::cloud_sync::transfer::{CloudTransfer, path_to_remote_key};
use crate::config::{Config, get_backup_path, get_config, set_config_local};
use crate::preclude::*;

fn acquire_error() -> BackendError {
    BackendError::Unexpected(anyhow::anyhow!("Semaphore closed unexpectedly"))
}

fn join_error(e: tokio::task::JoinError) -> BackendError {
    BackendError::Unexpected(anyhow::anyhow!("Task panicked: {e}"))
}

fn game_cloud_metadata_path(game_name: &str) -> Result<String, BackendError> {
    let backups_json = PathBuf::from("save_data")
        .join(game_name)
        .join("Backups.json");
    path_to_remote_key(&backups_json)
}

fn game_cloud_zip_path(game_name: &str, backup_date: &str) -> Result<String, BackendError> {
    let zip_path = PathBuf::from("save_data")
        .join(game_name)
        .join(format!("{backup_date}.zip"));
    path_to_remote_key(&zip_path)
}

pub async fn upload_all(op: &Operator, max_concurrency: usize) -> Result<(), BackendError> {
    let config = get_config()?;
    let backup_root = get_backup_path()?;

    upload_config(op).await?;

    let semaphore = Arc::new(Semaphore::new(max_concurrency.max(1)));
    let op = op.clone();

    for game in config.games {
        let backup_info = game.get_game_snapshots_info()?;
        let metadata_path = game_cloud_metadata_path(&game.name)?;
        let transfer = CloudTransfer::new(&op);
        transfer
            .upload_bytes_streaming(&serde_json::to_vec_pretty(&backup_info)?, &metadata_path)
            .await?;

        let mut tasks = JoinSet::new();
        for backup in backup_info.backups {
            let cloud_zip_path = game_cloud_zip_path(&game.name, &backup.date)?;
            let local_zip_path = if backup.path.is_empty() {
                backup_root
                    .join(&game.name)
                    .join(format!("{}.zip", backup.date))
            } else {
                PathBuf::from(&backup.path)
            };

            let permit = semaphore.clone().acquire_owned().await.map_err(|_| {
                acquire_error()
            })?;
            let op_clone = op.clone();
            tasks.spawn(async move {
                info!(
                    target:"rgsm::cloud::utils",
                    "Uploading {} from {}",
                    cloud_zip_path,
                    local_zip_path.display()
                );
                let transfer = CloudTransfer::new(&op_clone);
                let result = transfer
                    .upload_file_streaming(&local_zip_path, &cloud_zip_path)
                    .await;
                drop(permit);
                result
            });
        }

        while let Some(result) = tasks.join_next().await {
            result.map_err(join_error)??;
        }
    }

    Ok(())
}

pub async fn download_all(op: &Operator, max_concurrency: usize) -> Result<(), BackendError> {
    let op = op.clone();
    let transfer = CloudTransfer::new(&op);

    let config_bytes = transfer
        .download_bytes_streaming("/GameSaveManager.config.json")
        .await?;
    let config: Config = serde_json::from_slice(&config_bytes)?;
    set_config_local(&config)?;

    let backup_root = get_backup_path()?;
    let semaphore = Arc::new(Semaphore::new(max_concurrency.max(1)));

    for game in config.games {
        let metadata_path = game_cloud_metadata_path(&game.name)?;
        let metadata_bytes = transfer.download_bytes_streaming(&metadata_path).await?;
        let backup_info: GameSnapshots = serde_json::from_slice(&metadata_bytes)?;

        let local_game_backup_dir = backup_root.join(&game.name);
        fs::create_dir_all(&local_game_backup_dir).await?;

        // Download all zip files concurrently
        let mut tasks = JoinSet::new();
        for backup in &backup_info.backups {
            let cloud_zip_path = game_cloud_zip_path(&game.name, &backup.date)?;
            let local_zip_path = local_game_backup_dir.join(format!("{}.zip", backup.date));

            let permit = semaphore.clone().acquire_owned().await.map_err(|_| {
                acquire_error()
            })?;
            let op_clone = op.clone();
            tasks.spawn(async move {
                info!(
                    target:"rgsm::cloud::utils",
                    "Downloading {} to {}",
                    cloud_zip_path,
                    local_zip_path.display()
                );
                let transfer = CloudTransfer::new(&op_clone);
                let result = transfer
                    .download_file_streaming(&cloud_zip_path, &local_zip_path)
                    .await;
                drop(permit);
                result
            });
        }

        while let Some(result) = tasks.join_next().await {
            result.map_err(join_error)??;
        }

        // Update paths in metadata and write locally
        let mut updated_info = backup_info;
        for backup in &mut updated_info.backups {
            let local_zip_path = local_game_backup_dir.join(format!("{}.zip", backup.date));
            backup.path = local_zip_path.to_string_lossy().to_string();
        }

        transfer
            .write_local_bytes_atomically(
                &local_game_backup_dir.join("Backups.json"),
                &serde_json::to_vec_pretty(&updated_info)?,
            )
            .await?;
    }

    Ok(())
}

/// Upload metadata for one game.
pub async fn upload_game_snapshots(op: &Operator, info: GameSnapshots) -> Result<(), BackendError> {
    let transfer = CloudTransfer::new(op);
    let metadata_path = game_cloud_metadata_path(&info.name)?;
    transfer
        .upload_bytes_streaming(&serde_json::to_vec_pretty(&info)?, &metadata_path)
        .await?;
    Ok(())
}

/// Upload the whole config file.
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
