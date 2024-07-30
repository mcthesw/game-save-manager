use std::fs::File;
use std::{fs, path};

use rust_i18n::t;
use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::api::notification::Notification;
use tracing::info;

use crate::cloud::CloudSettings;
use crate::default_value;
use crate::errors::ConfigError;
use crate::traits::Sanitizable;

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
    #[serde(default = "default_value::default_false")]
    pub delete_before_apply: bool,
}

/// A game struct contains the save units and the game's launcher
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Game {
    pub name: String,
    pub save_paths: Vec<SaveUnit>,
    pub game_path: Option<String>,
}

/// Settings that can be configured by user
#[derive(Debug, Serialize, Deserialize, Clone)]
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
    #[serde(default = "default_value::default_locale")]
    pub locale: String,
    #[serde(default = "default_value::default_false")]
    pub default_delete_before_apply: bool,
    #[serde(default = "default_value::default_false")]
    pub default_expend_favorites_tree: bool,
    #[serde(default = "default_value::default_home_page")]
    pub home_page: String,
    #[serde(default = "default_value::default_true")]
    pub log_to_file: bool,
}

impl Sanitizable for Settings {
    fn sanitize(self) -> Self {
        Settings {
            cloud_settings: self.cloud_settings.sanitize(),
            ..self
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FavoriteTreeNode {
    node_id: String,
    label: String,
    is_leaf: bool,
    children: Option<Vec<Self>>,
}

/// The software's configuration
/// include the version, backup's location path, games'info,
/// and the settings
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub version: String,
    pub backup_path: String,
    pub games: Vec<Game>,
    pub settings: Settings,
    #[serde(default = "default_value::empty_vec")]
    pub favorites: Vec<FavoriteTreeNode>,
}

impl Sanitizable for Config {
    fn sanitize(self) -> Self {
        Config {
            settings: self.settings.sanitize(),
            ..self
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            version: String::from("1.3.0"),
            backup_path: String::from("./save_data"),
            games: Vec::new(),
            settings: Settings {
                prompt_when_not_described: false,
                extra_backup_when_apply: true,
                show_edit_button: false,
                prompt_when_auto_backup: true,
                cloud_settings: default_value::default_cloud_settings(),
                exit_to_tray: true,
                locale: default_value::default_locale(),
                default_delete_before_apply: false,
                default_expend_favorites_tree: false,
                home_page: default_value::default_home_page(),
                log_to_file: true,
            },
            favorites: vec![],
        }
    }
}

/// Set settings to original state
pub async fn reset_settings() -> Result<(), ConfigError> {
    let settings = Config::default().settings;
    let mut config = get_config()?;
    config.settings = settings;
    set_config(&config).await
}

/// Create a config file
fn init_config() -> Result<(), ConfigError> {
    info!("Init config file.");
    fs::write(
        "./GameSaveManager.config.json",
        serde_json::to_string_pretty(&Config::default())?,
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
pub fn config_check() -> Result<(), ConfigError> {
    let config_path = path::Path::new("./GameSaveManager.config.json");
    if !config_path.is_file() || !config_path.exists() {
        init_config()?;
    }
    let mut config = get_config()?;

    // 处理早期版本兼容性
    if config.version == "1.0.0 alpha" {
        "1.0.0-alpha".clone_into(&mut config.version);
    }
    let software_version = Version::parse(&Config::default().version)?;
    let config_version = Version::parse(&config.version)?;
    if config_version != software_version {
        Notification::new("Update Config Info")
            .title(t!("backend.config.updating_config_title"))
            .body(t!("backend.config.updating_config_body"))
            .show()
            .expect("Cannot show notification");
        backup_old_config()?;
    }
    if config_version < Version::parse("1.0.0")? {
        panic!("The config version is not supported.It's too old.")
    }
    if config_version < software_version {
        upgrade_config_version(&mut config, &software_version)?;
    }
    if config_version > software_version {
        panic!("The config version is higher than the software.")
    }

    rust_i18n::set_locale(&config.settings.locale);
    Ok(()) // return the config json
}

fn upgrade_config_version(
    config: &mut Config,
    software_version: &semver::Version,
) -> Result<(), ConfigError> {
    // 由于1.0之后版本保持了兼容性，因此不需要做任何处理，仅更新版本号并保存
    config.version = software_version.to_string();
    tauri::async_runtime::block_on(async { set_config(config).await })?;
    Ok(())
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
    use super::Config;
    use anyhow::Result;

    #[test]
    fn serialize_default_config() -> Result<()> {
        let config = Config::default();
        let json = serde_json::to_string_pretty(&config)?;
        println!("序列化得到:\n{}", json);
        Ok(())
    }
}
