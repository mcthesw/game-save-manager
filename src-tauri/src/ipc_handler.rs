use crate::backup::{Game, GameSnapshots};
use crate::cloud_sync::{self, Backend, upload_all};
use crate::config::{Config, QuickActionSoundPreferences, get_config};
use crate::device::{Device, get_current_device_id};
use crate::path_resolver;
use crate::preclude::*;
use crate::{backup, config, quick_actions, sound};

use anyhow::Result;
use log::{debug, error, info, warn};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, Window};
use tauri_plugin_dialog::DialogExt;
use tauri_specta::Event;

#[allow(non_camel_case_types)]
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub enum NotificationLevel {
    info,
    warning,
    error,
}
#[derive(Debug, Serialize, Deserialize, Clone, Type, Event)]
pub struct IpcNotification {
    pub level: NotificationLevel,
    pub title: String,
    pub msg: String,
}

#[tauri::command]
#[specta::specta]
pub async fn open_url(url: String) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Opening url: {}", url);
    open::that(url).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to open url: {:?}", e);
        e.to_string()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn open_file_or_folder(path: String) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Opening file or folder: {}", path);

    let config = get_config().map_err(|e| e.to_string())?;
    let path = path_resolver::resolve_path(&path, None, &config).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to resolve url: {:?}", e);
        e.to_string()
    })?;

    debug!(target:"rgsm::ipc", "Resolved url: {}", path.display());
    open::that(path).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to open file or folder: {:?}", e);
        e.to_string()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn choose_save_file(app: AppHandle) -> Result<String, String> {
    info!(target:"rgsm::ipc", "Opening file dialog.");
    if let Some(path) = app.dialog().file().blocking_pick_file() {
        info!(target:"rgsm::ipc","Successfully picked file: {:#?}",path);
        Ok(path.to_string())
    } else {
        warn!(target:"rgsm::ipc", "Failed to open dialog or user close the dialog.");
        Err("Failed to open dialog.".to_string())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn choose_save_dir(app: AppHandle) -> Result<String, String> {
    info!(target:"rgsm::ipc","Opening folder dialog.");
    if let Some(path) = app.dialog().file().blocking_pick_folder() {
        info!(target:"rgsm::ipc","Successfully picked folder: {:#?}",path);
        Ok(path.to_string())
    } else {
        warn!(target:"rgsm::ipc", "Failed to open dialog or user close the dialog.");
        Err("Failed to open dialog.".to_string())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn get_local_config() -> Result<Config, String> {
    info!(target:"rgsm::ipc", "Getting local config.");
    get_config().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn add_game(game: Game) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Adding game: {:?}", game);
    backup::create_game_backup(&game).await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to add game: {:?}", e);
        e.to_string()
    })?;
    info!(target:"rgsm::ipc", "Successfully added game: {:?}", game);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn restore_snapshot(game: Game, date: String, app: AppHandle) -> Result<(), String> {
    //handle_backup_err(game.restore_snapshot(&date,window), )
    info!(target:"rgsm::ipc", "Applying backup: {:?} for game: {:?}", date, game);
    game.restore_snapshot(&date, Some(&app)).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to apply backup: {:?}", e);
        e.to_string()
    })?;
    info!(target:"rgsm::ipc", "Successfully applied backup: {:?} for game: {:?}", date, game);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_snapshot(game: Game, date: String) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Deleting backup: {:?} for game: {:?}", date, game);
    game.delete_snapshot(&date).await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to delete backup: {:?}", e);
        e.to_string()
    })?;
    info!(target:"rgsm::ipc", "Successfully deleted backup: {:?} for game: {:?}", date, game);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_game(game: Game) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Deleting game: {:?}", game);
    game.delete_game().await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to delete game: {:?}", e);
        e.to_string()
    })?;
    info!(target:"rgsm::ipc", "Successfully deleted game: {:?}", game);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_game_snapshots_info(game: Game) -> Result<GameSnapshots, String> {
    info!(target:"rgsm::ipc", "Getting backup list info for game: {:?}", game);
    game.get_game_snapshots_info().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get backup list info: {:?}", e);
        e.to_string()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn set_config(config: Config) -> Result<(), String> {
    debug!(target:"rgsm::ipc", "Setting config: {:?}", config.clone().sanitize());
    config::set_config(&config).await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to set config: {:?}", e);
        e.to_string()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn reset_settings() -> Result<(), String> {
    info!(target:"rgsm::ipc", "Resetting settings.");
    config::reset_settings().await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to reset settings: {:?}", e);
        e.to_string()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn create_snapshot(game: Game, describe: String, window: Window) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Backing up save for game: {:?}", game);
    handle_backup_err(game.create_snapshot(&describe).await, window)?;
    info!(target:"rgsm::ipc", "Successfully backed up save for game: {:?}", game);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn open_backup_folder(game: Game) -> Result<bool, String> {
    info!(target:"rgsm::ipc", "Opening backup folder for game: {:?}", game);
    let config = get_config().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get config: {:?}", e);
        e.to_string()
    })?;
    let p = PathBuf::from(&config.backup_path).join(game.name);
    Ok(open::that(p).is_ok())
}

#[tauri::command]
#[specta::specta]
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

#[tauri::command]
#[specta::specta]
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

#[tauri::command]
#[specta::specta]
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

#[tauri::command]
#[specta::specta]
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

#[tauri::command]
#[specta::specta]
pub async fn backup_all() -> Result<(), String> {
    info!(target:"rgsm::ipc","Backing up all games.");
    backup::backup_all().await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to backup all games: {:?}", e);
        e.to_string()
    })?;
    info!(target:"rgsm::ipc","Successfully backed up all games.");
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn apply_all(app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::ipc","Applying all backups.");
    backup::apply_all(Some(&app_handle)).await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to apply all backups: {:?}", e);
        e.to_string()
    })?;
    info!(target:"rgsm::ipc","Successfully applied all backups.");
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_quick_backup_game(app_handle: AppHandle, game: Game) -> Result<(), String> {
    info!(target:"rgsm::ipc","Setting quick backup game to: {:?}", game);
    let manager_state: tauri::State<Arc<quick_actions::QuickActionManager>> = app_handle.state();
    let manager = Arc::clone(manager_state.inner());
    manager
        .set_quick_backup_game(game.clone())
        .await
        .map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to set quick backup game: {:?}", e);
            e.to_string()
        })?;
    info!(target:"rgsm::ipc","Successfully set quick backup game to: {:?}", game);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_quick_action_sound_preview(
    app: AppHandle,
    preferences: QuickActionSoundPreferences,
    effect: sound::QuickActionSoundEffect,
) -> Result<(), String> {
    let manager = app.state::<sound::SoundManager>();
    manager
        .toggle_preview(preferences, effect)
        .await
        .map_err(|err| {
            error!(target: "rgsm::sound", "Failed to preview sound: {err:?}");
            err.to_string()
        })
}

#[tauri::command]
#[specta::specta]
pub async fn stop_sound_playback(app: AppHandle) -> Result<(), String> {
    let manager = app.state::<sound::SoundManager>();
    manager.stop().await.map_err(|err| {
        error!(target: "rgsm::sound", "Failed to stop sound: {err:?}");
        err.to_string()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn choose_quick_action_sound_file(app: AppHandle) -> Result<String, String> {
    sound::choose_quick_action_sound_file(&app)
}

/// Resolves a path string containing variables to an actual filesystem path
///
/// This command allows the frontend to resolve paths with variables like <home>, <winAppData>, etc.
#[tauri::command]
#[specta::specta]
pub async fn resolve_path(path: String) -> Result<String, String> {
    info!(target:"rgsm::ipc", "Resolving path: {}", path);

    let config = get_config().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get config: {:?}", e);
        e.to_string()
    })?;

    let resolved_path = path_resolver::resolve_path(&path, None, &config).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to resolve path: {:?}", e);
        e.to_string()
    })?;

    let path_str = resolved_path.to_str().ok_or_else(|| {
        let err = "Failed to convert resolved path to string";
        error!(target:"rgsm::ipc", "{}", err);
        err.to_string()
    })?;

    info!(target:"rgsm::ipc", "Successfully resolved path: {} -> {}", path, path_str);
    Ok(path_str.to_string())
}

/// Returns the current device, if not found, returns a default device
#[tauri::command]
#[specta::specta]
pub async fn get_current_device_info() -> Result<Device, String> {
    info!(target:"rgsm::ipc", "Getting current device info");

    let device_id = get_current_device_id();
    let config = get_config().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get config: {:?}", e);
        e.to_string()
    })?;

    Ok(config.devices.get(device_id).cloned().unwrap_or_default())
}

fn handle_backup_err(res: Result<(), BackupError>, window: Window) -> Result<(), String> {
    if let Err(e) = res {
        match &e {
            BackupError::Compress(CompressError::Multiple(files)) => {
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

#[cfg(test)]
mod test {
    use super::{IpcNotification, NotificationLevel};

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
