use std::path::PathBuf;

use crate::backup::BackupsInfo;
use crate::config::{config_check, get_config, Config, Game};
use crate::{backup, config};
use anyhow::Result;
use native_dialog::FileDialog;
use tauri::{AppHandle, Manager};

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
pub async fn apply_backup(game: Game, date: String) -> Result<(), String> {
    game.apply_backup(&date).map_err(|e| e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn delete_backup(game: Game, date: String,app_handle:AppHandle) -> Result<(), String> {
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
pub async fn backup_save(game: Game, describe: String) -> Result<(), String> {
    game.backup_save(&describe).map_err(|e| e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn open_backup_folder(game: Game) -> Result<bool, String> {
    let config = get_config().unwrap();
    let p = PathBuf::from(&config.backup_path).join(game.name);
    Ok(open::that(p).is_ok())
}
