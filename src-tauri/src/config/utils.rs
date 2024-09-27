use std::fs::File;
use std::{fs, path};

use rust_i18n::t;
use semver::Version;
use tauri::api::notification::Notification;
use tracing::info;

use super::Config;
use crate::errors::ConfigError;

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
