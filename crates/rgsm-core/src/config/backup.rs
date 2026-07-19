use std::fs;
use std::path::PathBuf;

use log::{info, warn};

use crate::config::Config;

const MAX_CONFIG_BACKUPS: usize = 5;
const CONFIG_FILE_NAME: &str = "GameSaveManager.config.json";

fn backup_path(index: usize) -> PathBuf {
    backup_path_in(crate::app_dirs::get_app_data_dir(), index)
}

fn backup_path_in(root: &std::path::Path, index: usize) -> PathBuf {
    root.join(format!("{CONFIG_FILE_NAME}.backup.{index}"))
}

/// Rotate local config backups before saving.
///
/// Keeps up to `MAX_CONFIG_BACKUPS` copies:
///   .backup.0 = most recent, .backup.4 = oldest.
/// Called before each config write so corruption is recoverable.
pub fn rotate_config_backups(config: &Config) {
    rotate_config_backups_in(crate::app_dirs::get_app_data_dir(), config);
}

fn rotate_config_backups_in(root: &std::path::Path, config: &Config) {
    // Shift existing backups: .backup.4 is dropped, .backup.3 → .backup.4, etc.
    for i in (1..MAX_CONFIG_BACKUPS).rev() {
        let src = backup_path_in(root, i - 1);
        let dst = backup_path_in(root, i);
        if src.is_file() {
            // On Windows, fs::rename fails if destination exists; remove first.
            if dst.exists() {
                let _ = fs::remove_file(&dst);
            }
            if let Err(e) = fs::rename(&src, &dst) {
                warn!(
                    "Failed to rotate config backup {} → {}: {e}",
                    src.display(),
                    dst.display()
                );
            }
        }
    }

    // Store the previous effective Config projection for the existing restore UI.
    let serialized = match serde_json::to_vec_pretty(config) {
        Ok(serialized) => serialized,
        Err(e) => {
            warn!("Failed to serialize config backup: {e}");
            return;
        }
    };
    if let Err(e) = fs::write(backup_path_in(root, 0), serialized) {
        warn!("Failed to create config backup: {e}");
    } else {
        info!("Config backup rotated (max {MAX_CONFIG_BACKUPS})");
    }
}

/// List available config backup files, ordered newest-first.
pub fn list_config_backups() -> Vec<PathBuf> {
    (0..MAX_CONFIG_BACKUPS)
        .map(backup_path)
        .filter(|p| p.is_file())
        .collect()
}

/// Load the effective config projection stored in a specific backup.
pub fn load_config_from_backup(index: usize) -> Result<Config, std::io::Error> {
    load_config_from_backup_in(crate::app_dirs::get_app_data_dir(), index)
}

fn load_config_from_backup_in(
    root: &std::path::Path,
    index: usize,
) -> Result<Config, std::io::Error> {
    let src = backup_path_in(root, index);
    if !src.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Backup index {index} does not exist"),
        ));
    }
    // Validate the backup is parseable JSON before restoring
    let content = fs::read_to_string(&src)?;
    let config = serde_json::from_str::<Config>(&content).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Backup {index} is corrupt: {e}"),
        )
    })?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_path_format() {
        let p = backup_path(0);
        assert!(
            p.to_string_lossy()
                .contains("GameSaveManager.config.json.backup.0")
        );
    }

    #[test]
    fn test_list_empty_when_no_backups() {
        // list_config_backups should not panic even when no backup files exist
        let backups = list_config_backups();
        assert!(backups.len() <= MAX_CONFIG_BACKUPS);
    }

    #[test]
    fn effective_config_backup_round_trips_without_legacy_file() {
        let root = temp_dir::TempDir::new().unwrap();
        let config = Config {
            backup_path: "device-specific-root".to_string(),
            ..Default::default()
        };

        rotate_config_backups_in(root.path(), &config);
        let restored = load_config_from_backup_in(root.path(), 0).unwrap();

        assert_eq!(restored.backup_path, "device-specific-root");
        assert!(!root.path().join("GameSaveManager.config.json").exists());
    }
}
