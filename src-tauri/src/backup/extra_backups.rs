use serde::{Deserialize, Serialize};
use specta::Type;
use std::{ffi::OsStr, fs, path::PathBuf, time::SystemTime};
use tauri::AppHandle;

use crate::config::get_backup_path;
use crate::preclude::*;

use super::{Game, decompress_from_file};

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct ExtraBackupItem {
    /// Filename without extension, e.g. `Overwrite_2025-12-22_12-34-56`.
    pub date: String,
    pub size: u64,
    /// File modification time in milliseconds since Unix epoch.
    pub modified_time_ms: Option<i64>,
}

pub fn extra_backup_folder_path(game: &Game) -> Result<PathBuf, BackupError> {
    Ok(get_backup_path()?.join(&game.name).join("extra_backup"))
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

    // Sort by modified time ascending (oldest first) to make retention cleanup predictable.
    items.sort_by(|a, b| match (a.modified_time_ms, b.modified_time_ms) {
        (Some(a_ms), Some(b_ms)) => a_ms.cmp(&b_ms).then_with(|| a.date.cmp(&b.date)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.date.cmp(&b.date),
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
    app_handle: Option<&AppHandle>,
) -> Result<(), BackupError> {
    let dir = extra_backup_folder_path(game)?;
    decompress_from_file(&game.save_paths, &dir, date, app_handle)?;
    Ok(())
}

fn system_time_to_ms(t: SystemTime) -> Option<i64> {
    let d = t.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    i64::try_from(d.as_millis()).ok()
}
