use std::fs::File;
use std::{fs, path};

use serde::{Deserialize, Serialize};
use tauri::api::notification::Notification;

use crate::cloud::CloudSettings;
use crate::default_value;
use crate::errors::ConfigError;

/// A save unit should be a file or a folder
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum SaveUnitType {
    File,
    Folder,
}

/// A save unit declares one of the files/folders
/// that should be backup for a game
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SaveUnit {
    pub unit_type: SaveUnitType,
    pub path: String,
}

/// A game struct contains the save units and the game's launcher
#[derive(Debug, Serialize, Deserialize)]
pub struct Game {
    pub name: String,
    pub save_paths: Vec<SaveUnit>,
    pub game_path: Option<String>,
}

impl Clone for Game {
    fn clone(&self) -> Self {
        Game {
            name: self.name.clone(),
            save_paths: self.save_paths.clone(),
            game_path: self.game_path.clone(),
        }
    }
}

/// Settings that can be configured by user
#[derive(Debug, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_value::default_true")]
    pub prompt_when_not_described: bool,
    #[serde(default = "default_value::default_true")]
    pub extra_backup_when_apply: bool,
    #[serde(default = "default_value::default_false")]
    pub show_edit_button: bool,
    #[serde(default = "default_value::default_true")]
    pub prompt_when_auto_backup: bool,
    #[serde(default = "default_value::default_true")]
    pub exit_to_tray: bool,
    #[serde(default = "default_value::default_cloud_settings")]
    pub cloud_settings: CloudSettings,
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
        version: String::from("1.0.1"),
        backup_path: String::from("./save_data"),
        games: Vec::new(),
        settings: Settings {
            prompt_when_not_described: false,
            extra_backup_when_apply: true,
            show_edit_button: false,
            prompt_when_auto_backup: true,
            cloud_settings: default_value::default_cloud_settings(),
            exit_to_tray: true,
        },
    }
}

/// Set settings to original state
pub async fn reset_settings() -> Result<(), ConfigError> {
    let settings = default_config().settings;
    let mut config = get_config()?;
    config.settings = settings;
    set_config(&config).await
}

/// Create a config file
fn init_config() -> Result<(), ConfigError> {
    println!("Init config file.");
    fs::write(
        "./GameSaveManager.config.json",
        serde_json::to_string_pretty(&default_config())?,
    )?;
    Ok(())
}

/// Get the current config file
pub fn get_config() -> Result<Config, ConfigError> {
    let file = File::open("./GameSaveManager.config.json")?;
    Ok(serde_json::from_reader(file)?)
}

/// Replace the config file with a new config struct
pub async fn set_config(config: &Config) -> Result<(), ConfigError> {
    fs::write(
        "./GameSaveManager.config.json",
        serde_json::to_string_pretty(&config)?,
    )?;
    // 处理云同步，上传新的配置文件
    if config.settings.cloud_settings.always_sync {
        let op = config.settings.cloud_settings.backend.get_op()?;
        crate::cloud::upload_config(&op).await?;
    }
    Ok(())
}

/// Check the config file exists or not
/// if not, then create one
/// then send the config to the front end
pub async fn config_check() -> Result<(), ConfigError> {
    let config_path = path::Path::new("./GameSaveManager.config.json");
    if !config_path.is_file() || !config_path.exists() {
        init_config()?;
    }
    let mut config = get_config()?;
    if config.version != default_config().version {
        Notification::new("Update Config Info")
            .title("更新配置文件")
            .body("配置文件已更新，旧的将会备份为 GameSaveManager.config.json.bak")
            .show()
            .expect("Cannot show notification");
        backup_old_config()?;
        if config.version == "1.0.0 alpha" {
            // 没有破坏性变化，可以直接采用默认值
            config.version = "1.0.0".to_owned();
        }
        if config.version == "1.0.0" {
            // 没有破坏性变化，可以直接采用默认值
            config.version = "1.0.1".to_owned();
        }
        set_config(&config).await?;
    }
    Ok(()) // return the config json
}

fn backup_old_config() -> Result<(), ConfigError> {
    fs::copy(
        "./GameSaveManager.config.json",
        "./GameSaveManager.config.json.bak",
    )?;
    Ok(())
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
