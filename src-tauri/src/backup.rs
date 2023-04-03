use crate::archive;
use crate::config::{get_config, Game, SaveUnit, SaveUnitType};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path};

/// A backup is a zip file that contains
/// all the file that the save unit has declared.
/// The date is the unique indicator for a backup
#[derive(Debug, Serialize, Deserialize)]
struct Backup {
    date: String,
    describe: String,
    path: String, // like "D:\\SaveManager\save_data\Game1\date.zip"
}

/// A backups info is a json file in a backup folder for a game.
/// It contains the name of the game,
/// all backups' path
/// and the icon of the game
#[derive(Debug, Serialize, Deserialize)]
pub struct BackupsInfo {
    name: String,
    backups: Vec<Backup>,
    icon: String,
}

pub fn get_backups_info(name: &str) -> Result<BackupsInfo> {
    let config = get_config()?;
    let backup_path = path::Path::new(&config.backup_path).join(name);
    let backup_info = serde_json::from_slice(&fs::read(backup_path)?)?;
    Ok(backup_info)
}

pub fn set_backups_info(name: &str, new_info: BackupsInfo) -> Result<()> {
    let config = get_config()?;
    let saves_path = path::Path::new(&config.backup_path).join(name);
    fs::write(saves_path, serde_json::to_string_pretty(&new_info)?)?;
    Ok(())
}

pub fn backup_save(name: &str, describe: &str) -> Result<()> {
    let config = get_config()?;
    if let Some(game) = config.games.into_iter().find(move |x| x.name.eq(name)) {
        let backup_path = path::Path::new(&config.backup_path).join(name); // the backup zip file should be placed here
        let date = chrono::Local::now()
            .format("YYYY-MM-DD_HH-mm-ss")
            .to_string();
        let save_paths = game.save_paths; // everything you should copy
        archive::compress_to_file(save_paths, &backup_path, &date)?;
        Ok(())
    } else {
        Err(anyhow::anyhow!("Game not exists"))
    }
}
