use crate::backup::BackupsInfo;
use crate::config::{config_check, get_config, Config, Game};
use crate::errors::*;
use crate::{backup, config};
use anyhow::Result;
use native_dialog::FileDialog;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::Window;

#[allow(non_camel_case_types)]
#[derive(Debug, Serialize, Deserialize, Clone)]
enum NotificationLevel {
    info,
    warning,
    error,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
struct IpcNotification {
    level: NotificationLevel,
    title: String,
    msg: String,
}

// TODO:把错误文本改为有可读性的
#[allow(unused)]
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    open::that(url).map_err(|e| e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn choose_save_file() -> Result<String, String> {
    if let Ok(path) = FileDialog::new().show_open_single_file() {
        Ok(path
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .to_string())
    } else {
        Err("Failed to open dialog.".to_string())
    }
}

#[allow(unused)]
#[tauri::command]
pub async fn choose_save_dir() -> Result<String, String> {
    if let Ok(path) = FileDialog::new().show_open_single_dir() {
        Ok(path
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .to_string())
    } else {
        Err("Failed to open dialog.".to_string())
    }
}

#[allow(unused)]
#[tauri::command]
pub async fn get_local_config() -> Result<Config, String> {
    get_config().map_err(|e| e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn local_config_check() -> Result<(), String> {
    config_check().map_err(|e| e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn add_game(game: Game) -> Result<(), String> {
    backup::create_game_backup(game).map_err(|e| e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn apply_backup(game: Game, date: String, window: Window) -> Result<(), String> {
    handle_backup_err(game.apply_backup(&date), window)
}

#[allow(unused)]
#[tauri::command]
pub async fn delete_backup(game: Game, date: String) -> Result<(), String> {
    game.delete_backup(&date).map_err(|e| e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn delete_game(game: Game) -> Result<(), String> {
    game.delete().map_err(|e| e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn get_backups_info(game: Game) -> Result<BackupsInfo, String> {
    game.get_backups_info().map_err(|e| e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn set_config(config: Config) -> Result<(), String> {
    config::set_config(config).map_err(|e| e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn reset_settings() -> Result<(), String> {
    config::reset_settings().map_err(|e| e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn backup_save(game: Game, describe: String, window: Window) -> Result<(), String> {
    handle_backup_err(game.backup_save(&describe), window)
}

#[allow(unused)]
#[tauri::command]
pub async fn open_backup_folder(game: Game) -> Result<bool, String> {
    let config = get_config().unwrap();
    let p = PathBuf::from(&config.backup_path).join(game.name);
    Ok(open::that(p).is_ok())
}

fn handle_backup_err(res: Result<(), BackupZipError>, window: Window) -> Result<(), String> {
    if let Err(e) = res {
        if let BackupZipError::NotExists(files) = &e {
            files.iter().for_each(|file| {
                window
                    .emit(
                        "Notification",
                        IpcNotification {
                            level: NotificationLevel::error,
                            title: "文件不存在".to_string(),
                            msg: format!("文件 {:?} 不存在，无法进行备份或恢复", file),
                        },
                    )
                    .unwrap();
            });
        }
        return Err(format!("{}", e));
    }
    Ok(())
}

mod test {
    use super::*;

    #[test]
    fn test1() {
        let a = serde_json::to_string(&IpcNotification {
            level: NotificationLevel::error,
            title: "title1".to_string(),
            msg: "msg1".to_string()
        }).unwrap();
        assert_eq!(a,"{\"level\":\"error\",\"title\":\"title1\",\"msg\":\"msg1\"}")
    }
}
