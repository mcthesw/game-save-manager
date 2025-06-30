use rust_i18n::t;
use std::fs;
use std::path::{Path, PathBuf};

use log::{error, info, warn};
use semver::Version;

use crate::config::Config;
use crate::preclude::*;
use crate::updater::{
    probe::probe_config_version,
    versions::{CURRENT_VERSION, Config1_4_0, MIN_SUPPORTED_VERSION, VERSION_1_4_0},
};

/// Update configuration file to the latest version
///
/// This function handles the entire migration process:
/// 1. Version probing
/// 2. Version compatibility check
/// 3. Backup creation
/// 4. Data migration
/// 5. New config writing
///
/// # Arguments
/// * `path` - Path to the config file
///
/// # Returns
/// * `Ok(())` - If migration succeeds or not needed
/// * `Err(UpdaterError)` - If any step fails
pub fn update_config<P: AsRef<Path>>(path: P) -> Result<(), UpdaterError> {
    let path: &Path = path.as_ref();
    let version = probe_config_version(path)?;
    let current = Version::parse(CURRENT_VERSION)?;
    let min_supported = Version::parse(MIN_SUPPORTED_VERSION)?;

    // Version compatibility check
    if version > current {
        error!(target: "rgsm::updater", "Config version too new: {} > {}", version, current);
        return Err(UpdaterError::ConfigVersionTooNew);
    }
    if version < min_supported {
        error!(target: "rgsm::updater", "Config version too old: {} < {}", version, min_supported);
        return Err(UpdaterError::ConfigVersionTooOld);
    }
    if version == current {
        return Ok(());
    }

    warn!(target: "rgsm::updater", "Config version is older than current version, updating...");
    // Create backup
    backup_config(path)?;

    // Read original content
    let content = fs::read_to_string(path)?;

    // Migrate based on version
    let new_cfg = migrate_config(&content, &version)?;

    // Write new config
    fs::write(path, serde_json::to_string_pretty(&new_cfg)?)?;
    info!(target: "rgsm::updater", "Config updated successfully to version {}", CURRENT_VERSION);
    Ok(())
}

/// Migrate config content based on its version
fn migrate_config(content: &str, version: &Version) -> Result<Config, UpdaterError> {
    if version.to_string().as_str() <= VERSION_1_4_0 {
        let old_cfg: Config1_4_0 = serde_json::from_str(content)?;
        Ok(Config::from(old_cfg))
    } else {
        // Try direct deserialization for compatible versions
        let mut new_cfg: Config = serde_json::from_str(content)?;
        new_cfg.version = CURRENT_VERSION.to_string();
        Ok(new_cfg)
    }
}

/// Create a backup of the config file
fn backup_config<P: AsRef<Path>>(path: P) -> Result<PathBuf, UpdaterError> {
    let path = path.as_ref();
    let backup_path = path.with_extension("json.bak");

    // Show notification
    show_notification(
        t!("backend.config.updating_config_title"),
        t!("backend.config.updating_config_body"),
    );

    // Create backup
    fs::copy(path, &backup_path)?;
    info!(target: "rgsm::updater", "Created backup at {:?}", backup_path);

    Ok(backup_path)
}
