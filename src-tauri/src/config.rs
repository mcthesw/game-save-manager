use std::fs::File;
use std::{fs, path};

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// A save unit should be a file or a folder
#[derive(Debug, Serialize, Deserialize)]
pub enum SaveUnitType {
    File,
    Folder,
}

/// A save unit declares one of the files/folders
/// that should be backup for a game
#[derive(Debug, Serialize, Deserialize)]
pub struct SaveUnit {
    pub unit_type: SaveUnitType,
    pub path: String,
}

/// A game struct contains the save units and the game's launcher
#[derive(Debug, Serialize, Deserialize)]
pub struct Game {
    pub name:String,
    pub save_paths: Vec<SaveUnit>,
    pub game_path: Option<String>,
}

/// Settings that can be configured by user
#[derive(Debug, Serialize, Deserialize)]
pub struct Settings {
    prompt_when_not_described: bool,
    extra_backup_when_apply: bool,
}

/// The software's configuration
/// include the version, backup's location path, games'info,
/// and the settings
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub version: String,
    pub backup_path: String,
    pub games: Vec<Game>,
    pub settings: Settings,
}

/// Get the default config struct
fn default_config() -> Config {
    Config {
        version: String::from("0.4.0"),
        backup_path: String::from("./save_data"),
        games: Vec::new(),
        settings: Settings {
            prompt_when_not_described: false,
            extra_backup_when_apply: true,
        },
    }
}

/// Create a config file
fn init_config() -> Result<()> {
    println!("初始化配置文件");
    fs::write(
        "./GameSaveManager.config.json",
        serde_json::to_string_pretty(&default_config())?,
    )?;
    Ok(())
}

/// Get the current config file
pub fn get_config() -> Result<Config> {
    let file = File::open("./GameSaveManager.config.json")?;
    Ok(serde_json::from_reader(file)?)
}

/// Replace the config file with a new config struct
pub fn set_config(config: Config) -> Result<()> {
    fs::write(
        "./GameSaveManager.config.json",
        serde_json::to_string_pretty(&config)?,
    )?;
    Ok(())
}

/// Check the config file exists or not
/// if not, then create one
/// then send the config to the front end
#[tauri::command]
async fn config_check() -> Result<String> {
    let config_path = path::Path::new("./GameSaveManager.config.json");
    if !config_path.is_file() || !config_path.exists() {
        init_config()?;
    }
    let config = get_config()?;
    if config.version != default_config().version {
        //TODO:需要完成旧版本到新版本的迁移
        todo!();
    }
    Ok(serde_json::to_string(&config)?) // return the config json
}

#[cfg(test)]
mod test {
    use super::{default_config, Game, SaveUnit, SaveUnitType};
    use anyhow::Result;

    #[test]
    fn serialize_default_config() -> Result<()> {
        let config = default_config();
        let json = serde_json::to_string_pretty(&config)?;
        println!("序列化得到:\n{}", json);
        Ok(())
    }
    #[test]
    fn serialize_games() -> Result<()> {
        let mut units = Vec::new();
        units.push(SaveUnit {
            unit_type: SaveUnitType::File,
            path: String::from("C://aaa.txt"),
        });
        units.push(SaveUnit {
            unit_type: SaveUnitType::Folder,
            path: String::from("C://aaa"),
        });
        let mut games = Vec::new();
        games.push(Game {
            name: String::from("111"),
            game_path: None,
            save_paths: units,
        });
        let json = serde_json::to_string(&games)?;
        assert_eq!(json,String::from(
            "[{\"name\":\"111\",\"save_paths\":[{\"unit_type\":\"File\",\"path\":\"C://aaa.txt\"},{\"unit_type\":\"Folder\",\"path\":\"C://aaa\"}],\"game_path\":null}]"
        ));
        Ok(())
    }
}
