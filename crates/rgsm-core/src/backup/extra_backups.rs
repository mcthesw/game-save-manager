use serde::{Deserialize, Serialize};
use specta::Type;
use std::{
    collections::HashMap,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::config::get_backup_path;
use crate::preclude::*;

use super::Game;

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

    let mut items_by_date: HashMap<String, (ExtraBackupItem, bool)> = HashMap::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if !is_extra_backup_archive(&path) {
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
        let item = ExtraBackupItem {
            date: file_stem,
            size: metadata.len(),
            modified_time_ms,
        };
        let is_v4 = path.extension() == Some(OsStr::new("7z"));
        items_by_date
            .entry(item.date.clone())
            .and_modify(|(existing, existing_is_v4)| {
                if is_v4 && !*existing_is_v4 {
                    *existing = item.clone();
                    *existing_is_v4 = true;
                }
            })
            .or_insert((item, is_v4));
    }

    let mut items = items_by_date
        .into_values()
        .map(|(item, _)| item)
        .collect::<Vec<_>>();
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
    for extension in ["7z", "zip"] {
        let path = dir.join(format!("{date}.{extension}"));
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

pub(super) fn cleanup_oldest_extra_backups(
    extra_backup_path: &Path,
    max_count: u32,
) -> Result<(), BackupError> {
    if max_count == 0 {
        return Ok(());
    }

    let mut items = extra_backup_path
        .read_dir()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if !is_extra_backup_archive(&path) {
                return None;
            }
            let file_name = path.file_name()?.to_os_string();
            let modified = fs::metadata(&path).ok()?.modified().ok()?;
            Some((modified, file_name))
        })
        .collect::<Vec<_>>();

    if items.len() <= max_count as usize {
        return Ok(());
    }

    items.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let to_delete = items.len() - max_count as usize;
    for (_, file_name) in items.into_iter().take(to_delete) {
        let _ = fs::remove_file(extra_backup_path.join(file_name));
    }
    Ok(())
}

fn is_extra_backup_archive(path: &Path) -> bool {
    matches!(
        path.extension(),
        Some(extension) if extension == OsStr::new("zip") || extension == OsStr::new("7z")
    )
}

fn system_time_to_ms(t: SystemTime) -> Option<i64> {
    let d = t.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    i64::try_from(d.as_millis()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_counts_zip_and_seven_z_toward_one_retention_limit() {
        let temp = temp_dir::TempDir::new().unwrap();
        let zip = temp.path().join("Overwrite_old.zip");
        let seven_z = temp.path().join("Overwrite_new.7z");
        fs::write(&zip, b"old").unwrap();
        fs::write(&seven_z, b"new").unwrap();
        filetime::set_file_mtime(&zip, filetime::FileTime::from_unix_time(1_000, 0)).unwrap();
        filetime::set_file_mtime(&seven_z, filetime::FileTime::from_unix_time(2_000, 0)).unwrap();

        cleanup_oldest_extra_backups(temp.path(), 1).unwrap();

        assert!(!zip.exists());
        assert!(seven_z.exists());
    }
}
