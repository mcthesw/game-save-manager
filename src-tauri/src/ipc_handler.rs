use crate::backup::{Game, GameSnapshots};
use crate::cloud_sync::{self, upload_all, Backend};
use crate::config::{get_config, Config};
use crate::traits::Sanitizable;
use crate::{backup, config};
use crate::{errors::*, tray};
use anyhow::Result;
use rust_embed::Embed;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::api::dialog;
use tauri::{AppHandle, Window};
use tracing::{debug, error, info, warn};

#[allow(non_camel_case_types)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum NotificationLevel {
    info,
    warning,
    error,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IpcNotification {
    pub level: NotificationLevel,
    pub title: String,
    pub msg: String,
}

// 用于读取locale文件
#[derive(Embed)]
#[folder = "../locales/"]
#[prefix = "locales/"]
struct Asset;

#[allow(unused)]
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Opening url: {}", url);
    open::that(url).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to open url: {:?}", e);
        e.to_string()
    })
}

#[allow(unused)]
#[tauri::command]
pub async fn choose_save_file() -> Result<String, String> {
    info!(target:"rgsm::ipc", "Opening file dialog.");
    if let Some(path) = dialog::blocking::FileDialogBuilder::new().pick_file() {
        info!(target:"rgsm::ipc","Successfully picked file: {:#?}",path);
        Ok(path.to_string_lossy().into_owned())
    } else {
        warn!(target:"rgsm::ipc", "Failed to open dialog or user close the dialog.");
        Err("Failed to open dialog.".to_string())
    }
}

#[allow(unused)]
#[tauri::command]
pub async fn choose_save_dir() -> Result<String, String> {
    info!(target:"rgsm::ipc","Opening folder dialog.");
    if let Some(path) = dialog::blocking::FileDialogBuilder::new().pick_folder() {
        info!(target:"rgsm::ipc","Successfully picked folder: {:#?}",path);
        Ok(path.to_string_lossy().into_owned())
    } else {
        warn!(target:"rgsm::ipc", "Failed to open dialog or user close the dialog.");
        Err("Failed to open dialog.".to_string())
    }
}

#[allow(unused)]
#[tauri::command]
pub async fn get_local_config() -> Result<Config, String> {
    info!(target:"rgsm::ipc", "Getting local config.");
    get_config().map_err(|e| e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn add_game(game: Game) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Adding game: {:?}", game);
    backup::create_game_backup(&game).await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to add game: {:?}", e);
        e.to_string()
    })?;
    info!(target:"rgsm::ipc", "Successfully added game: {:?}", game);
    Ok(())
}

#[allow(unused)]
#[tauri::command]
pub async fn restore_snapshot(
    game: Game,
    date: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    //handle_backup_err(game.restore_snapshot(&date,window), )
    info!(target:"rgsm::ipc", "Applying backup: {:?} for game: {:?}", date, game);
    game.restore_snapshot(&date, &app_handle).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to apply backup: {:?}", e);
        e.to_string()
    })?;
    info!(target:"rgsm::ipc", "Successfully applied backup: {:?} for game: {:?}", date, game);
    Ok(())
}

#[allow(unused)]
#[tauri::command]
pub async fn delete_snapshot(game: Game, date: String) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Deleting backup: {:?} for game: {:?}", date, game);
    game.delete_snapshot(&date).await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to delete backup: {:?}", e);
        e.to_string()
    })?;
    info!(target:"rgsm::ipc", "Successfully deleted backup: {:?} for game: {:?}", date, game);
    Ok(())
}

#[allow(unused)]
#[tauri::command]
pub async fn delete_game(game: Game) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Deleting game: {:?}", game);
    game.delete_game().await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to delete game: {:?}", e);
        e.to_string()
    })?;
    info!(target:"rgsm::ipc", "Successfully deleted game: {:?}", game);
    Ok(())
}

#[allow(unused)]
#[tauri::command]
pub async fn get_game_snapshots_info(game: Game) -> Result<GameSnapshots, String> {
    info!(target:"rgsm::ipc", "Getting backup list info for game: {:?}", game);
    game.get_game_snapshots_info().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get backup list info: {:?}", e);
        e.to_string()
    })
}

#[allow(unused)]
#[tauri::command]
pub async fn set_config(config: Config) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Setting config: {:?}", config.clone().sanitize());
    config::set_config(&config).await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to set config: {:?}", e);
        e.to_string()
    })
}

#[allow(unused)]
#[tauri::command]
pub async fn reset_settings() -> Result<(), String> {
    info!(target:"rgsm::ipc", "Resetting settings.");
    config::reset_settings().await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to reset settings: {:?}", e);
        e.to_string()
    })
}

#[allow(unused)]
#[tauri::command]
pub async fn create_snapshot(game: Game, describe: String, window: Window) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Backing up save for game: {:?}", game);
    handle_backup_err(game.create_snapshot(&describe).await, window)?;
    info!(target:"rgsm::ipc", "Successfully backed up save for game: {:?}", game);
    Ok(())
}

#[allow(unused)]
#[tauri::command]
pub async fn open_backup_folder(game: Game) -> Result<bool, String> {
    info!(target:"rgsm::ipc", "Opening backup folder for game: {:?}", game);
    let config = get_config().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get config: {:?}", e);
        e.to_string()
    })?;
    let p = PathBuf::from(&config.backup_path).join(game.name);
    Ok(open::that(p).is_ok())
}

#[allow(unused)]
#[tauri::command]
pub async fn check_cloud_backend(backend: Backend) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Checking cloud backend: {:?}", backend.clone().sanitize());
    match backend.check().await {
        Ok(_) => {
            info!(target:"rgsm::ipc", "Successfully checked cloud backend: {:?}", backend.sanitize());
            Ok(())
        }
        Err(e) => {
            error!(target:"rgsm::ipc", "Failed to check cloud backend: {:?}", e);
            Err(e.to_string())
        }
    }
}

#[allow(unused)]
#[tauri::command]
pub async fn cloud_upload_all(backend: Backend) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Uploading all backups to cloud backend: {:?}", backend.clone().sanitize());
    let op = backend.get_op().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get cloud backend operator: {:?}", e);
        e.to_string()
    })?;
    match upload_all(&op).await {
        Ok(_) => {
            info!(target:"rgsm::ipc", "Successfully uploaded all backups to cloud backend: {:?}", backend.sanitize());
            Ok(())
        }
        Err(e) => {
            error!(target:"rgsm::ipc", "Failed to upload all backups to cloud backend: {:?}", e);
            Err(e.to_string())
        }
    }
}

#[allow(unused)]
#[tauri::command]
pub async fn cloud_download_all(backend: Backend) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Downloading all backups from cloud backend: {:?}", backend.clone().sanitize());
    let op = backend.get_op().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get cloud backend operator: {:?}", e);
        e.to_string()
    })?;
    match cloud_sync::download_all(&op).await {
        Ok(_) => {
            info!(target:"rgsm::ipc", "Successfully downloaded all backups from cloud backend: {:?}", backend.sanitize());
            Ok(())
        }
        Err(e) => {
            error!(target:"rgsm::ipc", "Failed to download all backups from cloud backend: {:?}", e);
            Err(e.to_string())
        }
    }
}

#[allow(unused)]
#[tauri::command]
pub async fn set_snapshot_description(
    game: Game,
    date: String,
    describe: String,
) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Setting backup describe for game: {:?}", game);
    game.set_snapshot_description(&date, &describe)
        .await
        .map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to set backup describe: {:?}", e);
            e.to_string()
        })?;
    info!(target:"rgsm::ipc", "Successfully set backup {} describe for game: {:?}", date,game);
    Ok(())
}

#[allow(unused)]
#[tauri::command]
pub async fn backup_all() -> Result<(), String> {
    info!(target:"rgsm::ipc","Backing up all games.");
    backup::backup_all().await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to backup all games: {:?}", e);
        e.to_string()
    })?;
    info!(target:"rgsm::ipc","Successfully backed up all games.");
    Ok(())
}

#[allow(unused)]
#[tauri::command]
pub async fn apply_all(app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::ipc","Applying all backups.");
    backup::apply_all(&app_handle).await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to apply all backups: {:?}", e);
        e.to_string()
    })?;
    info!(target:"rgsm::ipc","Successfully applied all backups.");
    Ok(())
}

#[allow(unused)]
#[tauri::command]
pub async fn set_quick_backup_game(app_handle: AppHandle, game: Game) -> Result<(), String> {
    info!(target:"rgsm::ipc","Setting quick backup game to: {:?}", game);
    tray::set_current_game(&app_handle, game);
    Ok(())
}

#[allow(unused)]
#[tauri::command]
pub async fn get_locale_message(
    handle: tauri::AppHandle,
) -> Result<HashMap<String, serde_json::Value>, String> {
    info!(target:"rgsm::ipc","Loading locale files");
    let mut map = HashMap::new();
    // 需要在此处加入所有可以本地化的文件
    let locales = ["zh_SIMPLIFIED".to_owned(), "en_US".to_owned()];
    info!(target:"rgsm::ipc","Locales to be loaded: {:?}", locales);

    for locale in &locales {
        match Asset::get(&format!("locales/{}.json", locale)) {
            Some(embed_file) => {
                debug!(target:"rgsm::ipc","Found locale file for: {}", locale);
                let file_str = match std::str::from_utf8(embed_file.data.as_ref()) {
                    Ok(s) => s,
                    Err(e) => {
                        error!(target:"rgsm::ipc","Failed to convert locale file to string for {}: {}", locale, e);
                        return Err(e.to_string());
                    }
                };
                let locale_json: serde_json::Value = match serde_json::from_str(file_str) {
                    Ok(v) => v,
                    Err(e) => {
                        error!(target:"rgsm::ipc","Failed to parse locale JSON for {}: {}", locale, e);
                        return Err(e.to_string());
                    }
                };
                map.insert(locale.to_owned(), locale_json);
                debug!(target:"rgsm::ipc","Successfully loaded locale file for: {}", locale);
            }
            None => {
                error!(target:"rgsm::ipc","Locale file not found for: {}", locale);
                return Err("Cannot read locale file".to_owned());
            }
        }
    }

    Ok(map)
}

fn handle_backup_err(res: Result<(), BackupError>, window: Window) -> Result<(), String> {
    if let Err(e) = res {
        match &e {
            BackupError::CompressError(CompressError::Multiple(files)) => {
                files.iter().for_each(|file| {
                    error!(target:"rgsm::ipc","{}",file);
                    if let BackupFileError::NotExists(path) = file {
                        window
                            .emit(
                                "Notification",
                                IpcNotification {
                                    level: NotificationLevel::error,
                                    title: "ERROR".to_string(),
                                    msg: t!(
                                        "backend.backup.backup_file_not_exist",
                                        name = path.to_str().unwrap_or("Cannot get path")
                                    )
                                    .to_string(),
                                },
                            )
                            .unwrap(); // safe: ipc方法通过前端调用，此时window必然存在
                    }
                });
            }
            other => {
                error!(target:"rgsm::ipc","{}",other);
            }
        }
        return Err(format!("{}", e));
    }
    Ok(())
}

mod test {
    #[allow(unused)]
    use super::*;

    #[test]
    fn test1() {
        let a = serde_json::to_string(&IpcNotification {
            level: NotificationLevel::error,
            title: "title1".to_string(),
            msg: "msg1".to_string(),
        })
        .unwrap(); // safe:测试代码，不应出现错误，可以直接unwrap
        assert_eq!(
            a,
            "{\"level\":\"error\",\"title\":\"title1\",\"msg\":\"msg1\"}"
        )
    }
}
