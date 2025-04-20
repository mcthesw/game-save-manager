use std::fs;

use log::info;
use opendal::Operator;

use crate::backup::GameSnapshots;
use crate::config::{Config, get_config, set_config};
use crate::preclude::*;

pub async fn upload_all(op: &Operator) -> Result<(), BackendError> {
    let config = get_config()?;
    // 上传配置文件
    upload_config(op).await?;
    // 依次上传所有游戏的存档记录和存档
    for game in config.games {
        // !NOTICE: 这个地方必须硬编码，因为云端目录必须固定
        let cloud_backup_path = format!("save_data/{}", game.name);
        let backup_info = game.get_game_snapshots_info()?;
        // 写入存档记录
        op.write(
            &format!("{}/Backups.json", &cloud_backup_path),
            serde_json::to_string_pretty(&backup_info)?,
        )
        .await?;
        // 写入存档zip文件（不包括额外备份）
        for backup in backup_info.backups {
            // TODO: 此处的cloud_backup_path应当改为本地的路径
            let save_path = format!("{}/{}.zip", &cloud_backup_path, backup.date);
            info!(target:"rgsm::cloud::utils","Uploading {}", save_path);
            op.write(&save_path, fs::read(&save_path)?).await?;
        }
    }
    Ok(())
}

pub async fn download_all(op: &Operator) -> Result<(), BackendError> {
    // 下载配置文件
    let config = String::from_utf8(op.read("/GameSaveManager.config.json").await?.to_vec())?;
    let config: Config = serde_json::from_str(&config)?;
    set_config(&config).await?;
    // 依次下载所有游戏的存档记录和存档
    for game in config.games {
        // !NOTICE: 这个地方必须硬编码，因为云端目录必须固定
        let backup_path = format!("save_data/{}", game.name);
        let backup_info = op
            .read(&format!("{}/Backups.json", &backup_path))
            .await?
            .to_vec();
        let backup_info: GameSnapshots = serde_json::from_str(&String::from_utf8(backup_info)?)?;
        game.set_game_snapshots_info(&backup_info)?;
        // 写入存档记录
        // TODO: 此处的cloud_backup_path应当改为本地的路径
        fs::write(
            format!("{}/Backups.json", &backup_path),
            serde_json::to_string_pretty(&backup_info)?,
        )?;
        // 写入存档zip文件（不包括额外备份）
        for backup in backup_info.backups {
            let save_path = format!("{}/{}.zip", &backup_path, backup.date);
            info!(target:"rgsm::cloud::utils","Downloading {}", save_path);
            let data = op.read(&save_path).await?.to_vec();
            fs::write(&save_path, &data)?;
        }
    }
    Ok(())
}

/// 上传单个游戏的配置文件
pub async fn upload_game_snapshots(op: &Operator, info: GameSnapshots) -> Result<(), BackendError> {
    // !NOTICE: 这个地方必须硬编码，因为云端目录必须固定
    let backup_path = format!("save_data/{}", info.name);
    op.write(
        &format!("{}/Backups.json", &backup_path),
        serde_json::to_string_pretty(&info)?,
    )
    .await?;
    Ok(())
}

// 上传配置文件
pub async fn upload_config(op: &Operator) -> Result<(), BackendError> {
    // !NOTICE: 这个地方必须硬编码，因为云端目录必须固定
    let config = get_config()?;
    // 上传配置文件
    op.write(
        "/GameSaveManager.config.json",
        serde_json::to_string_pretty(&config)?,
    )
    .await?;
    Ok(())
}
