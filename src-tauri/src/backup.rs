use crate::archive::{compress_to_file, decompress_from_file};
use crate::config::{get_config, set_config, Game};
use crate::errors::BackupZipError;
use anyhow::{Ok, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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

impl Game {
    pub fn get_backups_info(&self) -> Result<BackupsInfo> {
        let config = get_config()?;
        let backup_path = path::Path::new(&config.backup_path)
            .join(&self.name)
            .join("Backups.json");
        let backup_info = serde_json::from_slice(&fs::read(backup_path)?)?;
        Ok(backup_info)
    }
    pub fn set_backups_info(&self, new_info: BackupsInfo) -> Result<()> {
        let config = get_config()?;
        let saves_path = path::Path::new(&config.backup_path)
            .join(&self.name)
            .join("Backups.json");
        fs::write(saves_path, serde_json::to_string_pretty(&new_info)?)?;
        Ok(())
    }
    pub fn backup_save(&self, describe: &str) -> Result<(), BackupZipError> {
        let config = get_config()?;
        let backup_path = path::Path::new(&config.backup_path).join(&self.name); // the backup zip file should be placed here
        let date = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let save_paths = &self.save_paths; // everything you should copy

        let zip_path = backup_path.join([&date, ".zip"].concat());
        if let Err(e) = compress_to_file(save_paths, &zip_path){
            // delete the zip if failed to write
            fs::remove_file(&zip_path)?;
            return Err(e)
        }
            

        let backups_info = Backup {
            date,
            describe: describe.to_string(),
            path: zip_path.to_str().unwrap().to_string(),
        };
        let mut infos = self.get_backups_info()?;
        infos.backups.push(backups_info);
        self.set_backups_info(infos)?;
        Result::Ok(())
    }
    pub fn apply_backup(&self, save_date: &str) -> Result<(), BackupZipError> {
        let config = get_config()?;
        let backup_path = path::Path::new(&config.backup_path).join(&self.name);
        if config.settings.extra_backup_when_apply {
            self.create_extra_backup()?;
        }
        decompress_from_file(&self.save_paths, &backup_path, save_date)?;
        Result::Ok(())
    }
    pub fn create_extra_backup(&self) -> Result<(), BackupZipError> {
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
                extra_backups.push(f?.file_name().into_string().unwrap());
                Ok(())
            })?;
            extra_backups.sort();
            let oldest = extra_backups.first().unwrap(); // 一定要改好这一行
            println!("oldest{:?}", oldest);
            fs::remove_file(extra_backup_path.join(oldest))?;
        }
        Result::Ok(())
    }
    pub fn delete_backup(&self, date: &str) -> Result<()> {
        let config = get_config()?;
        let save_path = PathBuf::from(&config.backup_path)
            .join(&self.name)
            .join(date.to_string() + ".zip");
        fs::remove_file(save_path)?;

        let mut saves = self.get_backups_info()?;
        saves.backups.retain(|x| x.date != date);
        self.set_backups_info(saves)?;
        Ok(())
    }
    pub fn delete(&self) -> Result<()> {
        let mut config = get_config()?;
        let backup_path = PathBuf::from(&config.backup_path).join(&self.name);
        fs::remove_dir_all(backup_path)?;

        config.games.retain(|x| x.name != self.name);
        set_config(config)?;
        Ok(())
    }
}

fn create_backup_folder(name: &str) -> Result<()> {
    let config = get_config()?;
    let info = BackupsInfo {
        name: name.to_string(),
        backups: Vec::new(),
    };
    let backup_path = PathBuf::from(&config.backup_path).join(name);
    if !backup_path.exists() {
        fs::create_dir_all(&backup_path)?;
    }
    fs::write(
        backup_path.join("Backups.json"),
        serde_json::to_string_pretty(&info)?,
    )?;

    Ok(())
}

pub fn create_game_backup(game: Game) -> Result<()> {
    let mut config = get_config()?;
    create_backup_folder(&game.name)?;
    config.games.push(game);
    set_config(config)?;
    Ok(())
}
