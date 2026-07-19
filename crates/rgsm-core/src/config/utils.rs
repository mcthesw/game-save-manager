use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex as StdMutex};
#[cfg(test)]
use tokio::sync::{Mutex as TokioMutex, MutexGuard};

use crate::app_dirs::resolve_app_path;
use crate::config::owner_store::OwnerStore;
use crate::config::{
    CloudNamespaceGeneration, Config, DeviceProfile, LocalState, SharedLibrary, backup,
};
use crate::preclude::*;
use crate::updater::update_config;
use log::info;

#[cfg(test)]
static CONFIG_FILE_TEST_LOCK: LazyLock<TokioMutex<()>> = LazyLock::new(|| TokioMutex::new(()));
static CONFIG_STORE_LOCK: LazyLock<StdMutex<()>> = LazyLock::new(|| StdMutex::new(()));

#[cfg(test)]
pub(crate) fn lock_config_test_file() -> MutexGuard<'static, ()> {
    CONFIG_FILE_TEST_LOCK.blocking_lock()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigCheckOutcome {
    pub config_migrated: bool,
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
    let _guard = CONFIG_STORE_LOCK
        .lock()
        .map_err(|_| ConfigError::StoreLockPoisoned)?;
    get_config_unlocked()
}

pub fn cloud_namespace_generation() -> Result<CloudNamespaceGeneration, ConfigError> {
    let _guard = CONFIG_STORE_LOCK
        .lock()
        .map_err(|_| ConfigError::StoreLockPoisoned)?;
    Ok(OwnerStore::runtime()
        .load()?
        .local_state
        .cloud_namespace_generation)
}

pub(crate) fn cloud_bootstrap_inputs()
-> Result<(SharedLibrary, DeviceProfile, LocalState), ConfigError> {
    let _guard = CONFIG_STORE_LOCK
        .lock()
        .map_err(|_| ConfigError::StoreLockPoisoned)?;
    let owners = OwnerStore::runtime().load()?;
    let profile = owners
        .device_profiles
        .get(&owners.local_state.current_device_id)
        .cloned()
        .ok_or_else(|| {
            crate::config::OwnershipError::MissingDeviceProfile(
                owners.local_state.current_device_id.clone(),
            )
        })
        .map_err(crate::config::OwnerStoreError::from)?;
    Ok((owners.shared_library, profile, owners.local_state))
}

pub(crate) fn activate_cloud_namespace_v2(
    expected_library: &SharedLibrary,
    expected_profile: &DeviceProfile,
) -> Result<(), ConfigError> {
    let _guard = CONFIG_STORE_LOCK
        .lock()
        .map_err(|_| ConfigError::StoreLockPoisoned)?;
    OwnerStore::runtime().activate_v2(expected_library, expected_profile)?;
    Ok(())
}

pub(crate) fn activate_joined_cloud_library(
    expected_local_library: &SharedLibrary,
    expected_local_profile: &DeviceProfile,
    accepted_library: &SharedLibrary,
    accepted_profile: &DeviceProfile,
) -> Result<(), ConfigError> {
    let _guard = CONFIG_STORE_LOCK
        .lock()
        .map_err(|_| ConfigError::StoreLockPoisoned)?;
    OwnerStore::runtime().activate_join_v2(
        expected_local_library,
        expected_local_profile,
        accepted_library,
        accepted_profile,
    )?;
    Ok(())
}

pub(crate) fn activate_cutover_cloud_library(
    expected_local_library: &SharedLibrary,
    expected_local_profile: &DeviceProfile,
    accepted_library: &SharedLibrary,
    accepted_profiles: &std::collections::HashMap<crate::device::DeviceId, DeviceProfile>,
) -> Result<(), ConfigError> {
    let _guard = CONFIG_STORE_LOCK
        .lock()
        .map_err(|_| ConfigError::StoreLockPoisoned)?;
    OwnerStore::runtime().activate_cutover_v2(
        expected_local_library,
        expected_local_profile,
        accepted_library,
        accepted_profiles,
    )?;
    Ok(())
}

pub(crate) fn replace_current_device_profile(
    expected: &DeviceProfile,
    accepted: &DeviceProfile,
) -> Result<(), ConfigError> {
    let _guard = CONFIG_STORE_LOCK
        .lock()
        .map_err(|_| ConfigError::StoreLockPoisoned)?;
    OwnerStore::runtime().replace_current_profile(expected, accepted)?;
    Ok(())
}

fn get_config_unlocked() -> Result<Config, ConfigError> {
    let owner_store = OwnerStore::runtime();
    if owner_store.has_authoritative_state() {
        return Ok(owner_store.load_effective()?);
    }
    let config_path = resolve_app_path("GameSaveManager.config.json");
    let content = fs::read_to_string(config_path)?;
    Ok(serde_json::from_str(&content)?)
}

/// Replace the config file with a new config struct without triggering cloud sync.
pub fn set_config_local(config: &Config) -> Result<(), ConfigError> {
    let _guard = CONFIG_STORE_LOCK
        .lock()
        .map_err(|_| ConfigError::StoreLockPoisoned)?;
    let mut normalized = config.clone();
    for game in &mut normalized.games {
        game.normalize_save_unit_ids();
    }
    if let Ok(previous) = get_config_unlocked() {
        backup::rotate_config_backups(&previous);
    }
    OwnerStore::runtime().merge_effective(&normalized)?;
    Ok(())
}

/// Replace every local owner from one effective Config.
///
/// This is intentionally narrower than `set_config_local`: only explicit V1
/// remote acceptance, import, or recovery flows should replace other Devices'
/// Profiles.
pub fn replace_config_local(config: &Config) -> Result<(), ConfigError> {
    let _guard = CONFIG_STORE_LOCK
        .lock()
        .map_err(|_| ConfigError::StoreLockPoisoned)?;
    let mut normalized = config.clone();
    for game in &mut normalized.games {
        game.normalize_save_unit_ids();
    }
    if let Ok(previous) = get_config_unlocked() {
        backup::rotate_config_backups(&previous);
    }
    OwnerStore::runtime().replace_effective(&normalized)?;
    Ok(())
}

/// Replace the config file with a new config struct
pub async fn set_config(config: &Config) -> Result<(), ConfigError> {
    set_config_local(config)?;
    // Cloud sync upload is now handled by the hook pipeline (CloudSyncEnqueueHook).
    Ok(())
}

/// Check the config file exists or not
/// if not, then create one
/// then send the config to the front end
pub fn config_check() -> Result<ConfigCheckOutcome, ConfigError> {
    let _guard = CONFIG_STORE_LOCK
        .lock()
        .map_err(|_| ConfigError::StoreLockPoisoned)?;
    let owner_store = OwnerStore::runtime();
    if owner_store.has_authoritative_state() {
        let config = owner_store.load_effective()?;
        rust_i18n::set_locale(&config.settings.locale);
        return Ok(ConfigCheckOutcome {
            config_migrated: false,
        });
    }

    let config_path = resolve_app_path("GameSaveManager.config.json");
    info!("Config file path: {}", config_path.display());

    if !config_path.is_file() || !config_path.exists() {
        init_config()?;
    }
    // 执行配置迁移与升级
    let config_migrated = update_config(&config_path)?;
    let content = fs::read_to_string(&config_path)?;
    let config: Config = serde_json::from_str(&content)?;
    owner_store.initialize_from_legacy(&config)?;
    let config = owner_store.load_effective()?;
    // 应用本地化语言
    rust_i18n::set_locale(&config.settings.locale);
    Ok(ConfigCheckOutcome { config_migrated })
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
        assert_eq!(
            old_resolved, new_resolved,
            "Old default path './save_data' should resolve to same location as new default 'save_data'"
        );

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
            assert!(
                resolved.ends_with(expected_end),
                "Path '{}' should end with '{}'",
                input,
                expected_end
            );
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
        assert_eq!(
            resolved,
            PathBuf::from(absolute_path),
            "Absolute paths should be preserved as-is"
        );
    }

    #[test]
    fn test_config_path_formats_compatibility() {
        // Test that different path formats work correctly
        let formats = vec![
            "save_data",    // New default
            "./save_data",  // Old default
            "save_data/",   // With trailing slash
            "./save_data/", // Old with trailing slash
        ];

        for format in formats {
            let resolved = resolve_backup_path(format);
            // All should resolve to paths containing "save_data"
            let path_str = resolved.to_string_lossy();
            assert!(
                path_str.contains("save_data"),
                "Format '{}' should resolve to path containing 'save_data', got: {}",
                format,
                path_str
            );
        }
    }
}
