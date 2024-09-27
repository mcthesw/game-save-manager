use serde::{Deserialize, Serialize};

use crate::cloud_sync::{upload_config, upload_game_snapshots};
use crate::config::{get_config, set_config};
use crate::errors::BackupError;
use crate::ipc_handler::{IpcNotification, NotificationLevel};
use std::path::PathBuf;
use std::{fs, path};
use tauri::{AppHandle, Manager};
use tracing::{error, info};

use super::GameSnapshots;
use super::SaveUnit;
use super::Snapshot;
use super::{compress_to_file, decompress_from_file};

/// A game struct contains the save units and the game's launcher
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Game {
    pub name: String,
    pub save_paths: Vec<SaveUnit>,
    pub game_path: Option<String>,
}

impl Game {
    pub fn get_game_snapshots_info(&self) -> Result<GameSnapshots, BackupError> {
        let config = get_config()?;
        let backup_path = path::Path::new(&config.backup_path)
            .join(&self.name)
            .join("Backups.json");
        let backup_info = serde_json::from_slice(&fs::read(backup_path)?)?;
        Ok(backup_info)
    }
    pub fn set_game_snapshots_info(&self, new_info: &GameSnapshots) -> Result<(), BackupError> {
        let config = get_config()?;
        let saves_path = path::Path::new(&config.backup_path)
            .join(&self.name)
            .join("Backups.json");
        // 处理文件夹不存在的情况，一般发生在初次下载云存档时
        let prefix_root = saves_path.parent().ok_or(BackupError::NonePathError)?;
        if !prefix_root.exists() {
            fs::create_dir_all(prefix_root)?;
        }
        fs::write(saves_path, serde_json::to_string_pretty(&new_info)?)?;
        Ok(())
    }
    pub async fn create_snapshot(&self, describe: &str) -> Result<(), BackupError> {
        let config = get_config()?;
        let backup_path = path::Path::new(&config.backup_path).join(&self.name); // the backup zip file should be placed here
        let date = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let save_paths = &self.save_paths; // everything you should copy

        let zip_path = backup_path.join([&date, ".zip"].concat());
        if let Err(e) = compress_to_file(save_paths, &zip_path) {
            // delete the zip if failed to write
            fs::remove_file(&zip_path)?;
            return Err(BackupError::CompressError(e));
        }

        let game_snapshots_info = Snapshot {
            date,
            describe: describe.to_string(),
            path: zip_path
                .to_str()
                .ok_or(BackupError::NonePathError)?
                .to_string(),
        };
        let mut infos = self.get_game_snapshots_info()?;
        infos.backups.push(game_snapshots_info);
        self.set_game_snapshots_info(&infos)?;

        // 随时同步到云端
        if config.settings.cloud_settings.always_sync {
            let op = config.settings.cloud_settings.backend.get_op()?;
            // 上传存档记录信息
            upload_game_snapshots(&op, infos).await?;
            // 上传对应压缩包
            // 此处防止路径中出现反斜杠，导致云端无法识别，替换win的反斜杠为斜杠
            let p = zip_path
                .iter()
                .map(|s| s.to_str().ok_or(BackupError::NonePathError))
                .collect::<Result<Vec<&str>, BackupError>>()?
                .join("/");
            op.write(&p, fs::read(&zip_path)?).await?;
        }
        Result::Ok(())
    }
    pub fn restore_snapshot(&self, date: &str, app_handle: &AppHandle) -> Result<(), BackupError> {
        let config = get_config()?;
        let backup_path = path::Path::new(&config.backup_path).join(&self.name);
        if config.settings.extra_backup_when_apply {
            info!(target:"rgsm::backup::game","Creating extra backup.");
            if let Err(e) = self.create_overwrite_snapshot() {
                error!(target:"rgsm::backup::game","Failed to create extra backup: {:?}", e);
                app_handle
                    .emit_all(
                        "Notification",
                        IpcNotification {
                            level: NotificationLevel::error,
                            title: "ERROR".to_string(),
                            msg: t!("backend.backup.extra_backup_file_not_exist").to_string(),
                        },
                    )
                    .map_err(anyhow::Error::from)?;
                return Err(e);
            }
        }
        decompress_from_file(&self.save_paths, &backup_path, date, app_handle)?;
        Result::Ok(())
    }
    pub fn create_overwrite_snapshot(&self) -> Result<(), BackupError> {
        let config = get_config()?;
        let extra_backup_path = path::Path::new(&config.backup_path)
            .join(&self.name)
            .join("extra_backup");

        // Create extra backup
        if !extra_backup_path.exists() {
            fs::create_dir_all(&extra_backup_path)?;
        }
        let date = chrono::Local::now()
            .format("Overwrite_%Y-%m-%d_%H-%M-%S")
            .to_string();
        let zip_path = &extra_backup_path.join([&date, ".zip"].concat());
        compress_to_file(&self.save_paths, zip_path)?;

        // Delete oldest extra backup if there are more than 5 file
        let extra_backups_dir: Vec<_> = extra_backup_path.read_dir()?.collect();
        let mut extra_backups = Vec::new();
        if extra_backups_dir.len() >= 5 {
            extra_backups_dir.into_iter().try_for_each(|f| {
                extra_backups.push(
                    f?.file_name()
                        .to_str()
                        .ok_or(BackupError::NonePathError)?
                        .to_string(),
                );
                Result::<(), BackupError>::Ok(())
            })?;
            extra_backups.sort();
            let oldest = extra_backups.first().ok_or(BackupError::NonePathError)?; // 一定要改好这一行
            info!("Remove oldest: {:?}", oldest);
            fs::remove_file(extra_backup_path.join(oldest))?;
        }
        Result::Ok(())
    }
    pub async fn delete_snapshot(&self, date: &str) -> Result<(), BackupError> {
        let config = get_config()?;
        let save_path = PathBuf::from(&config.backup_path)
            .join(&self.name)
            .join(date.to_string() + ".zip");
        fs::remove_file(&save_path)?;

        let mut saves = self.get_game_snapshots_info()?;
        saves.backups.retain(|x| x.date != date);
        self.set_game_snapshots_info(&saves)?;

        // 随时同步到云端
        if config.settings.cloud_settings.always_sync {
            let op = config.settings.cloud_settings.backend.get_op()?;
            // 上传存档记录信息
            upload_game_snapshots(&op, saves).await?;
            // 删除对应压缩包
            // 此处防止路径中出现反斜杠，导致云端无法识别，替换win的反斜杠为斜杠
            let p = save_path
                .iter()
                .map(|s| s.to_str().ok_or(BackupError::NonePathError))
                .collect::<Result<Vec<&str>, BackupError>>()?
                .join("/");
            op.delete(&p).await?;
        }
        Ok(())
    }
    pub async fn delete_game(&self) -> Result<(), BackupError> {
        let mut config = get_config()?;
        let backup_path = PathBuf::from(&config.backup_path).join(&self.name);
        fs::remove_dir_all(&backup_path)?;

        config.games.retain(|x| x.name != self.name);
        set_config(&config).await?;

        // 随时同步到云端
        if config.settings.cloud_settings.always_sync {
            let op = config.settings.cloud_settings.backend.get_op()?;
            info!(target:"rgsm::backup::game",
                "Delete Game: {:#?}",
                backup_path.to_str().ok_or(BackupError::NonePathError)?
            );
            // 此处防止路径中出现反斜杠，导致云端无法识别，替换win的反斜杠为斜杠
            let p = backup_path
                .iter()
                .map(|s| s.to_str().ok_or(BackupError::NonePathError))
                .collect::<Result<Vec<&str>, BackupError>>()?
                .join("/");
            op.remove_all(&p).await?;
            // 也上传新的配置文件
            upload_config(&op).await?;
        }

        Ok(())
    }
    pub async fn set_snapshot_description(
        &self,
        date: &str,
        describe: &str,
    ) -> Result<(), BackupError> {
        let mut saves = self.get_game_snapshots_info()?;
        let pos = saves.backups.iter().position(|x| x.date == date).ok_or(
            BackupError::BackupNotExist {
                name: self.name.clone(),
                date: date.to_string(),
            },
        )?;
        saves.backups[pos].describe = describe.to_string();
        self.set_game_snapshots_info(&saves)?;
        Ok(())
    }
}
