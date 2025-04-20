use std::fs::File;
use std::{fs, path};

use crate::config::Config;
use crate::preclude::*;
use crate::updater::update_config;
use log::info;

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
        crate::cloud_sync::upload_config(&op).await?;
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
    // 执行配置迁移与升级
    update_config(config_path)?;
    // 重新加载配置
    let config = get_config()?;
    // 应用本地化语言
    rust_i18n::set_locale(&config.settings.locale);
    Ok(())
}
