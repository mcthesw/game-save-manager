use serde::{Deserialize, Serialize};
use specta::Type;
use std::{ffi::OsStr, fs, path::PathBuf, time::SystemTime};

use crate::backup::archive::RestoreNotifier;
use crate::config::{get_backup_path, get_config};
use crate::device::get_current_device_id;
use crate::preclude::*;

use super::{ArchiveBackend, Game, ZipBackend};

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct ExtraBackupItem {
    /// Filename without extension, e.g. `Overwrite_2025-12-22_12-34-56`.
    pub date: String,
    pub size: u64,
    /// File modification time in milliseconds since Unix epoch.
    pub modified_time_ms: Option<i64>,
}

pub fn extra_backup_folder_path(game: &Game) -> Result<PathBuf, BackupError> {
    Ok(get_backup_path()?
        .join(game.backup_dir_name().as_ref())
        .join("extra_backup"))
}

pub fn list_extra_backups(game: &Game) -> Result<Vec<ExtraBackupItem>, BackupError> {
    let dir = extra_backup_folder_path(game)?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension() != Some(OsStr::new("zip")) {
            continue;
        }

        let file_stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(v) => v.to_string(),
            None => continue,
        };
        if !file_stem.starts_with("Overwrite_") {
            continue;
        }

        let metadata = fs::metadata(&path)?;
        let modified_time_ms = metadata.modified().ok().and_then(system_time_to_ms);
        items.push(ExtraBackupItem {
            date: file_stem,
            size: metadata.len(),
            modified_time_ms,
        });
    }

    // Sort by modified time descending (newest first) for better UX.
    items.sort_by(|a, b| match (a.modified_time_ms, b.modified_time_ms) {
        (Some(a_ms), Some(b_ms)) => b_ms.cmp(&a_ms).then_with(|| b.date.cmp(&a.date)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => b.date.cmp(&a.date),
    });

    Ok(items)
}

pub fn delete_extra_backup(game: &Game, date: &str) -> Result<(), BackupError> {
    let dir = extra_backup_folder_path(game)?;
    let zip_path = dir.join(format!("{date}.zip"));
    if !zip_path.exists() {
        return Ok(());
    }
    fs::remove_file(zip_path)?;
    Ok(())
}

pub fn restore_extra_backup(
    game: &Game,
    date: &str,
    notifier: Option<&dyn RestoreNotifier>,
) -> Result<(), BackupError> {
    let dir = extra_backup_folder_path(game)?;
    let archive_path = dir.join(format!("{date}.zip"));
    let device = get_config()
        .ok()
        .and_then(|c| c.devices.get(get_current_device_id()).cloned());
    ZipBackend.decompress(
        &game.save_paths,
        &archive_path,
        notifier,
        Some(&game.path_context(device.as_ref())),
    )?;
    Ok(())
}

fn system_time_to_ms(t: SystemTime) -> Option<i64> {
    let d = t.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    i64::try_from(d.as_millis()).ok()
}
