use crate::archive::{self, compress_to_file, decompress_from_file};
use crate::config::{self, get_config, set_config, Game, SaveUnit, SaveUnitType};
use anyhow::{Ok, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
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
/// and all backups' path
#[derive(Debug, Serialize, Deserialize)]
pub struct BackupsInfo {
    name: String,
    backups: Vec<Backup>,
}

pub fn get_backups_info(game: &Game) -> Result<BackupsInfo> {
    let config = get_config()?;
    let backup_path = path::Path::new(&config.backup_path).join(game.name);
    let backup_info = serde_json::from_slice(&fs::read(backup_path)?)?;
    Ok(backup_info)
}

pub fn set_backups_info(game: &Game, new_info: BackupsInfo) -> Result<()> {
    let config = get_config()?;
    let saves_path = path::Path::new(&config.backup_path).join(game.name);
    fs::write(saves_path, serde_json::to_string_pretty(&new_info)?)?;
    Ok(())
}

pub fn backup_save(game: &Game, describe: &str) -> Result<()> {
    let config = get_config()?;
    let backup_path = path::Path::new(&config.backup_path).join(game.name); // the backup zip file should be placed here
    let date = chrono::Local::now()
        .format("YYYY-MM-DD_HH-mm-ss")
        .to_string();
    let save_paths = game.save_paths; // everything you should copy
    archive::compress_to_file(&save_paths, &backup_path, &date)?;
    Ok(())
}

pub fn apply_backup(game: &Game, save_date: &str) -> Result<()> {
    let config = get_config()?;
    let backup_path = path::Path::new(&config.backup_path).join(&game.name);
    decompress_from_file(&game.save_paths, &backup_path, &save_date)?;
    Ok(())
}

pub fn create_extra_backup(game: &Game) -> Result<()> {
    let config = get_config()?;
    let save_paths = game.save_paths;
    let extra_backup_path = path::Path::new(&config.backup_path)
        .join(game.name)
        .join("extra_backup");

    if !extra_backup_path.exists() {
        fs::create_dir_all(extra_backup_path)?;
    }

    let extra_backups = extra_backup_path.read_dir()?;
    if extra_backups.count() >= 5 {
        //FIXME:How to remove unwrap?
        let oldest = extra_backups
            .min_by(|x, y| {
                x.unwrap()
                    .metadata()
                    .unwrap()
                    .created()
                    .unwrap()
                    .cmp(&y.unwrap().metadata().unwrap().created().unwrap())
            })
            .unwrap()?;
        fs::remove_file(oldest.path())?;
    }
    let date = chrono::Local::now()
        .format("Overwrite:YYYY-MM-DD_HH-mm-ss")
        .to_string();
    compress_to_file(&game.save_paths, &extra_backup_path, &date)?;
    Ok(())
}

fn create_backup_folder(name: &str) -> Result<()> {
    let config = get_config()?;
    let info = BackupsInfo {
        name: name.to_string(),
        backups: Vec::new(),
    };
    let backup_path = PathBuf::from(&config.backup_path).join(name);
    if !backup_path.exists() {
        fs::create_dir_all(backup_path)?;
    }
    fs::write(
        backup_path.join("Backups.json"),
        serde_json::to_string_pretty(&info)?,
    )?;

    Ok(())
}

pub fn create_game_backup(name: &str, save_path: &str, game_path: Option<String>) -> Result<()> {
    let game = Game {
        name: name.to_string(),
        save_paths: Vec::new(),
        game_path,
    };
    let mut config = get_config()?;
    config.games.push(game);
    create_backup_folder(name);
    set_config(config);
    Ok(())
}

pub fn delete_backup(game: &Game, date: &str) -> Result<()> {
    let config = get_config()?;
    let save_path = PathBuf::from(&config.backup_path)
        .join(&game.name)
        .join(date.to_string() + ".zip");
    fs::remove_file(save_path)?;

    let mut saves = get_backups_info(game)?;
    saves.backups.retain(|&x| x.date == date);
    set_backups_info(game, saves);
    Ok(())
}

pub fn delete_game(game: &Game)->Result<()> {
    let config = get_config()?;
    let backup_path = PathBuf::from(&config.backup_path).join(&game.name);
    fs::remove_dir_all(backup_path)?;
    
    config.games.retain(|&x|x.name==game.name);
    set_config(config)?;
    Ok(())
}
