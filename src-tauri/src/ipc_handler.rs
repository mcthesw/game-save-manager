use std::path::PathBuf;

use crate::{backup, config};
use crate::config::{Config, config_check, Game, get_config};
use anyhow::{Result};
use native_dialog::{FileDialog};
use crate::backup::BackupsInfo;

#[allow(unused)]
#[tauri::command]
pub async fn open_url(url: String) -> bool {
    open::that(url).is_ok()
}

#[allow(unused)]
#[tauri::command]
pub async fn choose_save_file() -> Option<String> {
    if let Ok(path) = FileDialog::new().show_open_single_file() {
        Some(path
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .to_string())
    } else {
        None
    }
}

#[allow(unused)]
#[tauri::command]
pub async fn choose_save_dir() -> Option<String> {
    if let Ok(path) = FileDialog::new().show_open_single_dir() {
        Some(path
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .to_string())
    } else {
        None
    }
}

#[allow(unused)]
#[tauri::command]
pub async fn get_local_config() -> Result<Config,String> {
    match get_config() {
        // TODO:Handle old version config
        Ok(config)=>Ok(config),
        Err(e)=>Err(e.to_string())
    }
}

#[allow(unused)]
#[tauri::command]
pub async fn local_config_check() -> Result<(),String> {
    config_check().map_err(|e|e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn add_game(game:Game) -> Result<(),String> {
    backup::create_game_backup(game).map_err(|e|e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn apply_backup(game:Game,date:String) -> Result<(),String> {
    backup::apply_backup(&game,&date).map_err(|e|e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn apply_backup_with_extra_backup(game:Game,date:String) -> Result<(),String> {
    todo!() //TODO:增加该功能
}

#[allow(unused)]
#[tauri::command]
pub async fn delete_backup(game:Game,date:String) -> Result<(),String> {
    backup::delete_backup(&game,&date).map_err(|e|e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn delete_game(game:Game) -> Result<(),String> {
    backup::delete_game(&game).map_err(|e|e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn get_backups_info(game:Game) -> Result<BackupsInfo,String> {
    backup::get_backups_info(&game).map_err(|e|e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn set_config(config:Config) -> Result<(),String> {
    config::set_config(config).map_err(|e|e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn reset_settings() -> Result<(),String> {
    config::reset_settings().map_err(|e|e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn backup_save(game:Game,describe:String) -> Result<(),String> {
    backup::backup_save(&game,&describe).map_err(|e|e.to_string())
}

#[allow(unused)]
#[tauri::command]
pub async fn open_backup_folder(game: Game) -> bool {
    let config = get_config().unwrap();
    let p = PathBuf::from(&config.backup_path).join(game.name);
    open::that(p).is_ok()
}
