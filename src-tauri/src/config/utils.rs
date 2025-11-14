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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_old_config_backup_path_compatibility() {
        // Test that old default path "./save_data" resolves the same as new default "save_data"
        let old_default = "./save_data";
        let new_default = "save_data";
        
        let old_resolved = resolve_backup_path(old_default);
        let new_resolved = resolve_backup_path(new_default);
        
        // Both should resolve to the same location
        assert_eq!(old_resolved, new_resolved, 
            "Old default path './save_data' should resolve to same location as new default 'save_data'");
        
        // Both should end with "save_data"
        assert!(old_resolved.ends_with("save_data"));
        assert!(new_resolved.ends_with("save_data"));
    }
    
    #[test]
    fn test_backup_path_resolution_relative() {
        // Test various relative path formats
        let test_cases = vec![
            ("save_data", "save_data"),
            ("./save_data", "save_data"),
            ("backups/games", "games"),
            ("./backups/games", "games"),
        ];
        
        for (input, expected_end) in test_cases {
            let resolved = resolve_backup_path(input);
            assert!(resolved.ends_with(expected_end),
                "Path '{}' should end with '{}'", input, expected_end);
        }
    }
    
    #[test]
    fn test_backup_path_resolution_absolute() {
        // Test that absolute paths are preserved
        #[cfg(target_os = "windows")]
        let absolute_path = "C:\\Users\\Test\\Backups";
        #[cfg(not(target_os = "windows"))]
        let absolute_path = "/home/test/backups";
        
        let resolved = resolve_backup_path(absolute_path);
        assert_eq!(resolved, PathBuf::from(absolute_path),
            "Absolute paths should be preserved as-is");
    }
    
    #[test]
    fn test_config_path_formats_compatibility() {
        // Test that different path formats work correctly
        let formats = vec![
            "save_data",      // New default
            "./save_data",    // Old default
            "save_data/",     // With trailing slash
            "./save_data/",   // Old with trailing slash
        ];
        
        for format in formats {
            let resolved = resolve_backup_path(format);
            // All should resolve to paths containing "save_data"
            let path_str = resolved.to_string_lossy();
            assert!(path_str.contains("save_data"),
                "Format '{}' should resolve to path containing 'save_data', got: {}", 
                format, path_str);
        }
    }
}
