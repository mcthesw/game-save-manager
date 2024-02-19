use std::fs;

use crate::backup::BackupListInfo;
use crate::config::{get_config, set_config, Config};
use opendal::services;
use opendal::Operator;
use serde::{Deserialize, Serialize};

use crate::errors::BackendError;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Backend {
    // TODO:增加更多后端支持
    Disabled,
    WebDAV {
        endpoint: String,
        username: String,
        password: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CloudSettings {
    /// 是否启用跟随云同步（用户添加、删除时自动同步）
    pub always_sync: bool,
    /// 同步间隔，单位分钟，为0则不自动同步
    pub auto_sync_interval: u64,
    /// 云同步后端设置
    pub backend: Backend,
}

impl Backend {
    /// 获取 Operator 实例
    pub fn get_op(&self) -> Result<Operator, BackendError> {
        let mut builder = match self {
            Backend::Disabled => {
                return Err(BackendError::Disabled);
            }
            Backend::WebDAV {
                endpoint,
                username,
                password,
            } => {
                let mut builder = services::Webdav::default();
                builder.endpoint(endpoint);
                builder.username(username);
                builder.password(password);
                builder
            }
        };
        builder.root("/game-save-manager");
        Ok(Operator::new(builder)?.finish())
    }

    /// 检查后端是否可用
    pub async fn check(&self) -> Result<(), BackendError> {
        self.get_op()?.check().await?;
        Ok(())
    }
}

pub async fn upload_all(op: &Operator) -> Result<(), BackendError> {
    let config = get_config()?;
    // 上传配置文件
    upload_config(op).await?;
    // 依次上传所有游戏的存档记录和存档
    for game in config.games {
        let backup_path = format!("./save_data/{}", game.name);
        let backup_info = game.get_backup_list_info()?;
        // 写入存档记录
        op.write(
            &format!("{}/Backups.json", &backup_path),
            serde_json::to_string_pretty(&backup_info)?,
        )
        .await?;
        // 写入存档zip文件（不包括额外备份）
        for backup in backup_info.backups {
            let save_path = format!("{}/{}.zip", &backup_path, backup.date);
            println!("uploading {}", save_path);
            op.write(&save_path, fs::read(&save_path)?).await?;
        }
    }
    Ok(())
}

pub async fn download_all(op: &Operator) -> Result<(), BackendError> {
    // 下载配置文件
    let config = String::from_utf8(op.read("/GameSaveManager.config.json").await?)?;
    let config: Config = serde_json::from_str(&config)?;
    set_config(&config).await?;
    // 依次下载所有游戏的存档记录和存档
    for game in config.games {
        let backup_path = format!("./save_data/{}", game.name);
        let backup_info = op.read(&format!("{}/Backups.json", &backup_path)).await?;
        let backup_info: BackupListInfo = serde_json::from_str(&String::from_utf8(backup_info)?)?;
        game.set_backup_list_info(&backup_info)?;
        // 写入存档记录
        fs::write(
            &format!("{}/Backups.json", &backup_path),
            serde_json::to_string_pretty(&backup_info)?,
        )?;
        // 写入存档zip文件（不包括额外备份）
        for backup in backup_info.backups {
            let save_path = format!("{}/{}.zip", &backup_path, backup.date);
            println!("downloading {}", save_path);
            let data = op.read(&save_path).await?;
            fs::write(&save_path, &data)?;
        }
    }
    Ok(())
}

/// 上传单个游戏的配置文件
pub async fn upload_backup_info(op: &Operator, info: BackupListInfo) -> Result<(), BackendError> {
    let backup_path = format!("./save_data/{}", info.name);
    op.write(
        &format!("{}/Backups.json", &backup_path),
        serde_json::to_string_pretty(&info)?,
    )
    .await?;
    Ok(())
}

// 上传配置文件
pub async fn upload_config(op: &Operator) -> Result<(), BackendError> {
    let config = get_config()?;
    // 上传配置文件
    op.write(
        "/GameSaveManager.config.json",
        serde_json::to_string_pretty(&config)?,
    )
    .await?;
    Ok(())
}
