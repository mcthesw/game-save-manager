use std::fs::File;
use std::path::PathBuf;
use std::fs;

use crate::app_dirs::resolve_app_path;
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
    let config_path = resolve_app_path("GameSaveManager.config.json");
    info!("Init config file at: {}", config_path.display());
    fs::write(
        config_path,
        serde_json::to_string_pretty(&Config::default())?,
    )?;
    Ok(())
}

/// Get the current config file
pub fn get_config() -> Result<Config, ConfigError> {
    let config_path = resolve_app_path("GameSaveManager.config.json");
    let file = File::open(config_path)?;
    Ok(serde_json::from_reader(file)?)
}

/// Replace the config file with a new config struct
pub async fn set_config(config: &Config) -> Result<(), ConfigError> {
    let config_path = resolve_app_path("GameSaveManager.config.json");
    fs::write(
        config_path,
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
    let config_path = resolve_app_path("GameSaveManager.config.json");
    if !config_path.is_file() || !config_path.exists() {
        init_config()?;
    }
    // 执行配置迁移与升级
    update_config(&config_path)?;
    // 重新加载配置
    let config = get_config()?;
    // 应用本地化语言
    rust_i18n::set_locale(&config.settings.locale);
    Ok(())
}

/// Get the resolved backup path from the config
///
/// If the backup_path in config is relative, it will be resolved relative to the app data directory.
/// If it's absolute, it will be returned as-is.
pub fn get_backup_path() -> Result<PathBuf, ConfigError> {
    let config = get_config()?;
    Ok(resolve_backup_path(&config.backup_path))
}

/// Resolve a backup path
///
/// If the path is relative, resolve it relative to the app data directory.
/// If it's absolute, return it as-is.
pub fn resolve_backup_path(backup_path: &str) -> PathBuf {
    let path = PathBuf::from(backup_path);
    if path.is_absolute() {
        path
    } else {
        resolve_app_path(backup_path)
    }
}
