use rust_i18n::t;
use std::fs;
use std::path::{Path, PathBuf};

use log::{error, info, warn};
use semver::Version;

use crate::backup::GameSnapshots;
use crate::config::{get_backup_path, Config};
use crate::preclude::*;
use crate::updater::{
    probe::probe_config_version,
    versions::{Config1_4_0, CURRENT_VERSION, MIN_SUPPORTED_VERSION, VERSION_1_4_0, VERSION_1_6_0},
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
    let version_1_6_0 = Version::parse(VERSION_1_6_0)?;

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

    // Migrate game snapshots if upgrading from before 1.6.0
    if version < version_1_6_0 {
        migrate_game_snapshots_to_chain()?;
    }

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

/// Migrate game snapshots from flat list to chained structure (for versions < 1.6.0)
///
/// This function:
/// 1. Scans all game folders in the backup path
/// 2. For each Backups.json, sorts snapshots by date (ascending)
/// 3. Creates a parent chain from oldest to newest
/// 4. Sets head to the newest snapshot
fn migrate_game_snapshots_to_chain() -> Result<(), UpdaterError> {
    let backup_path = match get_backup_path() {
        Ok(p) => p,
        Err(e) => {
            warn!(target: "rgsm::updater", "Failed to get backup path, skipping snapshot migration: {}", e);
            return Ok(());
        }
    };

    if !backup_path.exists() {
        info!(target: "rgsm::updater", "Backup path does not exist, skipping snapshot migration");
        return Ok(());
    }

    // Iterate through all directories in backup path
    let entries = fs::read_dir(&backup_path)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let backups_json_path = path.join("Backups.json");
        if !backups_json_path.exists() {
            continue;
        }

        // Read the existing Backups.json
        let content = match fs::read_to_string(&backups_json_path) {
            Ok(c) => c,
            Err(e) => {
                warn!(target: "rgsm::updater", "Failed to read {:?}: {}", backups_json_path, e);
                continue;
            }
        };

        let mut game_snapshots: GameSnapshots = match serde_json::from_str(&content) {
            Ok(gs) => gs,
            Err(e) => {
                warn!(target: "rgsm::updater", "Failed to parse {:?}: {}", backups_json_path, e);
                continue;
            }
        };

        // Skip if already migrated (has parent or head set)
        if game_snapshots.head.is_some()
            || game_snapshots.backups.iter().any(|s| s.parent.is_some())
        {
            info!(target: "rgsm::updater", "Skipping {:?}: already has chain structure", backups_json_path);
            continue;
        }

        // Skip if no snapshots or only one snapshot
        if game_snapshots.backups.len() <= 1 {
            // For single snapshot, just set it as head
            if let Some(snapshot) = game_snapshots.backups.first() {
                game_snapshots.head = Some(snapshot.date.clone());
            }
        } else {
            // Sort snapshots by date (ascending - oldest first)
            game_snapshots.backups.sort_by(|a, b| a.date.cmp(&b.date));

            // Create parent chain: each snapshot points to the previous one
            for i in 1..game_snapshots.backups.len() {
                let parent_date = game_snapshots.backups[i - 1].date.clone();
                game_snapshots.backups[i].parent = Some(parent_date);
            }

            // Set head to the newest (last) snapshot
            if let Some(newest) = game_snapshots.backups.last() {
                game_snapshots.head = Some(newest.date.clone());
            }
        }

        // Write updated Backups.json
        match fs::write(
            &backups_json_path,
            serde_json::to_string_pretty(&game_snapshots)?,
        ) {
            Ok(_) => {
                info!(target: "rgsm::updater", "Migrated snapshots to chain structure: {:?}", backups_json_path);
            }
            Err(e) => {
                error!(target: "rgsm::updater", "Failed to write {:?}: {}", backups_json_path, e);
            }
        }
    }

    info!(target: "rgsm::updater", "Game snapshots migration completed");
    Ok(())
}
