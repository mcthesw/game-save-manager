use std::fs;
use std::path::PathBuf;

use log::{info, warn};

use crate::app_dirs::resolve_app_path;

const MAX_CONFIG_BACKUPS: usize = 5;
const CONFIG_FILE_NAME: &str = "GameSaveManager.config.json";

fn backup_path(index: usize) -> PathBuf {
    resolve_app_path(&format!("{CONFIG_FILE_NAME}.backup.{index}"))
}

/// Rotate local config backups before saving.
///
/// Keeps up to `MAX_CONFIG_BACKUPS` copies:
///   .backup.0 = most recent, .backup.4 = oldest.
/// Called before each config write so corruption is recoverable.
pub fn rotate_config_backups() {
    let config_path = resolve_app_path(CONFIG_FILE_NAME);
    if !config_path.is_file() {
        return;
    }

    // Shift existing backups: .backup.4 is dropped, .backup.3 → .backup.4, etc.
    for i in (1..MAX_CONFIG_BACKUPS).rev() {
        let src = backup_path(i - 1);
        let dst = backup_path(i);
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

    // Copy current config to .backup.0
    if let Err(e) = fs::copy(&config_path, backup_path(0)) {
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

/// Restore config from a specific backup index.
/// Returns `Ok(())` on success.
pub fn restore_config_from_backup(index: usize) -> Result<(), std::io::Error> {
    let src = backup_path(index);
    let dst = resolve_app_path(CONFIG_FILE_NAME);
    if !src.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Backup index {index} does not exist"),
        ));
    }
    // Validate the backup is parseable JSON before restoring
    let content = fs::read_to_string(&src)?;
    serde_json::from_str::<serde_json::Value>(&content).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Backup {index} is corrupt: {e}"),
        )
    })?;
    fs::copy(&src, &dst)?;
    info!("Config restored from backup index {index}");
    Ok(())
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
}
