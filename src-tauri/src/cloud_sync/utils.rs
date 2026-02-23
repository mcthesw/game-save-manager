use std::path::PathBuf;

use log::info;
use opendal::Operator;
use tokio::fs;

use crate::backup::GameSnapshots;
use crate::cloud_sync::transfer::{CloudTransfer, path_to_remote_key};
use crate::config::{Config, get_backup_path, get_config, set_config};
use crate::preclude::*;

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

pub async fn upload_all(op: &Operator) -> Result<(), BackendError> {
    let config = get_config()?;
    let transfer = CloudTransfer::new(op);
    let backup_root = get_backup_path()?;

    upload_config(op).await?;

    for game in config.games {
        let backup_info = game.get_game_snapshots_info()?;
        let metadata_path = game_cloud_metadata_path(&game.name)?;
        transfer
            .upload_bytes_streaming(&serde_json::to_vec_pretty(&backup_info)?, &metadata_path)
            .await?;

        for backup in backup_info.backups {
            let cloud_zip_path = game_cloud_zip_path(&game.name, &backup.date)?;
            let local_zip_path = if backup.path.is_empty() {
                backup_root
                    .join(&game.name)
                    .join(format!("{}.zip", backup.date))
            } else {
                PathBuf::from(&backup.path)
            };

            info!(
                target:"rgsm::cloud::utils",
                "Uploading {} from {}",
                cloud_zip_path,
                local_zip_path.display()
            );
            transfer
                .upload_file_streaming(&local_zip_path, &cloud_zip_path)
                .await?;
        }
    }

    Ok(())
}

pub async fn download_all(op: &Operator) -> Result<(), BackendError> {
    let transfer = CloudTransfer::new(op);

    let config_bytes = transfer
        .download_bytes_streaming("/GameSaveManager.config.json")
        .await?;
    let config: Config = serde_json::from_slice(&config_bytes)?;
    set_config(&config).await?;

    let backup_root = get_backup_path()?;

    for game in config.games {
        let metadata_path = game_cloud_metadata_path(&game.name)?;
        let metadata_bytes = transfer.download_bytes_streaming(&metadata_path).await?;
        let mut backup_info: GameSnapshots = serde_json::from_slice(&metadata_bytes)?;

        let local_game_backup_dir = backup_root.join(&game.name);
        fs::create_dir_all(&local_game_backup_dir).await?;

        for backup in &mut backup_info.backups {
            let cloud_zip_path = game_cloud_zip_path(&game.name, &backup.date)?;
            let local_zip_path = local_game_backup_dir.join(format!("{}.zip", backup.date));

            info!(
                target:"rgsm::cloud::utils",
                "Downloading {} to {}",
                cloud_zip_path,
                local_zip_path.display()
            );
            transfer
                .download_file_streaming(&cloud_zip_path, &local_zip_path)
                .await?;
            backup.path = local_zip_path.to_string_lossy().to_string();
        }

        transfer
            .write_local_bytes_atomically(
                &local_game_backup_dir.join("Backups.json"),
                &serde_json::to_vec_pretty(&backup_info)?,
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
