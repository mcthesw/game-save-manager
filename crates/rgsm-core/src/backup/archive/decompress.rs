//! ZIP archive extraction and save unit restoration.
//!
//! Handles all archive versions (Legacy, V1, V2) transparently.
//! V2 archives have index-prefixed entries that are mapped back to save units by stable ID.

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

use filetime::{FileTime, set_file_mtime};
use fs_extra::{dir::move_dir, file::move_file};
use log::warn;
use rust_i18n::t;

use crate::{
    backup::{SaveUnit, SaveUnitType},
    device::get_current_device_id,
    preclude::*,
};

/// Notification level for restore progress messages.
#[derive(Debug, Clone, Copy)]
pub enum RestoreNotificationLevel {
    Info,
    Warning,
}

/// Trait for receiving notifications during archive restoration.
///
/// GUI implements this to emit IPC events; CLI might log to stdout.
/// Passed as `Option<&dyn RestoreNotifier>` so callers without UI can pass `None`.
pub trait RestoreNotifier: Send + Sync {
    fn notify(&self, level: RestoreNotificationLevel, title: &str, msg: &str);
}

use super::{timestamp::zip_datetime_to_system_time, version::ArchiveVersion};

/// Why a save unit was skipped during restore.
#[derive(Debug, Clone)]
enum SkipReason {
    /// Archive predates the index-prefix format (registry units cannot be restored).
    LegacyArchiveFormat,
    /// The archive does not contain an entry for this save unit.
    MissingArchiveEntry,
    /// Registry operations are not supported on this platform.
    UnsupportedPlatform,
}

/// Outcome of restoring a single save unit.
#[derive(Debug)]
enum RestoreOutcome {
    Restored,
    Skipped(SkipReason),
}

fn emit_missing_path_warning(
    path: &Path,
    notifier: Option<&dyn RestoreNotifier>,
) -> Result<(), BackupFileError> {
    warn!(
        target:"rgsm::backup::archive",
        "Path {:#?} not exists, auto created",
        path.to_str().unwrap_or("path.to_str error")
    );

    if let Some(notifier) = notifier {
        let msg_path = path.to_str().unwrap_or("path.to_str error");
        notifier.notify(
            RestoreNotificationLevel::Warning,
            "WARNING",
            t!("backend.archive.file_not_exist", path = msg_path).as_ref(),
        );
    }

    Ok(())
}

fn extract_zip_entries_to_temp(
    zip: &mut zip::ZipArchive<File>,
    temp_root: &Path,
    version: ArchiveVersion,
) -> Result<Vec<(PathBuf, FileTime)>, CompressError> {
    let mut dir_timestamps = Vec::new();

    for index in 0..zip.len() {
        let mut zip_file = zip
            .by_index(index)
            .map_err(|e| CompressError::Single(e.into()))?;

        // Use enclosed_name() to prevent Zip Slip path traversal attacks.
        // Entries with names containing ".." or absolute paths are skipped.
        let safe_name = match zip_file.enclosed_name() {
            Some(name) => name.to_owned(),
            None => continue,
        };
        let out_path = temp_root.join(&safe_name);

        if zip_file.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| CompressError::Single(e.into()))?;
            if let Some(zip_time) = zip_file.last_modified() {
                let system_time = zip_datetime_to_system_time(zip_time, version);
                dir_timestamps.push((out_path, FileTime::from_system_time(system_time)));
            }
            continue;
        }

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| CompressError::Single(e.into()))?;
        }

        let mut output_file =
            File::create(&out_path).map_err(|e| CompressError::Single(e.into()))?;
        std::io::copy(&mut zip_file, &mut output_file)
            .map_err(|e| CompressError::Single(e.into()))?;
        drop(output_file);

        if let Some(zip_time) = zip_file.last_modified() {
            let system_time = zip_datetime_to_system_time(zip_time, version);
            let _ = set_file_mtime(&out_path, FileTime::from_system_time(system_time));
        }
    }

    Ok(dir_timestamps)
}

fn restore_file_unit(
    unit: &SaveUnit,
    original_path: PathBuf,
    target_path: PathBuf,
    notifier: Option<&dyn RestoreNotifier>,
) -> Result<(), BackupFileError> {
    let parent = target_path.parent().ok_or(BackupFileError::NonePathError)?;
    let restored_file_mtime = fs::metadata(&original_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(FileTime::from_system_time);

    if !parent.exists() {
        emit_missing_path_warning(parent, notifier)?;
        fs::create_dir_all(parent)?;
    }

    if unit.delete_before_apply && target_path.exists() {
        fs::remove_file(&target_path)?;
    }

    let option = fs_extra::file::CopyOptions::new().overwrite(true);
    move_file(original_path, &target_path, &option)?;

    if let Some(file_time) = restored_file_mtime {
        let _ = set_file_mtime(&target_path, file_time);
    }

    Ok(())
}

fn restore_folder_unit(
    unit: &SaveUnit,
    original_path: PathBuf,
    target_path: PathBuf,
    notifier: Option<&dyn RestoreNotifier>,
) -> Result<(), BackupFileError> {
    let parent = target_path.parent().ok_or(BackupFileError::NonePathError)?;

    if !parent.exists() {
        emit_missing_path_warning(parent, notifier)?;
        fs::create_dir_all(parent)?;
    }

    if unit.delete_before_apply && target_path.exists() {
        fs::remove_dir_all(&target_path)?;
    }

    let option = fs_extra::dir::CopyOptions::new().overwrite(true);
    move_dir(original_path, parent, &option)?;

    Ok(())
}

/// Emit a notification to the frontend indicating that a save unit was skipped during restore.
fn save_unit_label(unit: &SaveUnit) -> String {
    if let Some(path) = unit.paths.get(get_current_device_id()) {
        return path.clone();
    }

    let mut entries: Vec<_> = unit.paths.iter().collect();
    entries.sort_by(|(left_id, left_path), (right_id, right_path)| {
        left_id
            .cmp(right_id)
            .then_with(|| left_path.cmp(right_path))
    });

    entries
        .first()
        .map(|(_, path)| (*path).clone())
        .unwrap_or_else(|| format!("#{}", unit.id))
}

/// Emit a notification to the frontend indicating that a save unit was skipped during restore.
fn emit_skip_notification(
    unit: &SaveUnit,
    reason: &SkipReason,
    notifier: Option<&dyn RestoreNotifier>,
) {
    let unit_label = save_unit_label(unit);
    let reason_text = match reason {
        SkipReason::LegacyArchiveFormat => {
            t!("backend.archive.skip_reason_legacy_format").to_string()
        }
        SkipReason::MissingArchiveEntry => {
            t!("backend.archive.skip_reason_missing_entry").to_string()
        }
        SkipReason::UnsupportedPlatform => {
            t!("backend.archive.skip_reason_unsupported_platform").to_string()
        }
    };
    let msg = t!(
        "backend.archive.restore_unit_skipped",
        unit = unit_label.as_str(),
        reason = reason_text
    )
    .to_string();

    warn!(target: "rgsm::backup::archive", "{}", msg);

    if let Some(notifier) = notifier {
        notifier.notify(
            RestoreNotificationLevel::Info,
            t!("backend.archive.restore_skipped_title").as_ref(),
            &msg,
        );
    }
}

/// Restore a Windows Registry save unit from the extracted temp directory.
///
/// Reads `{id}/registry.json`, parses it, and imports the values back into
/// the Windows Registry. On non-Windows platforms the restore is skipped.
fn restore_registry_unit(
    unit: &SaveUnit,
    version: ArchiveVersion,
    temp_root: &Path,
) -> Result<RestoreOutcome, BackupFileError> {
    use crate::backup::registry;

    if !version.uses_save_unit_prefix() {
        return Ok(RestoreOutcome::Skipped(SkipReason::LegacyArchiveFormat));
    }

    let reg_json_path = temp_root
        .join(unit.id.to_string())
        .join(registry::REGISTRY_DATA_FILENAME);

    if !reg_json_path.exists() {
        return Ok(RestoreOutcome::Skipped(SkipReason::MissingArchiveEntry));
    }

    let json_bytes = fs::read(&reg_json_path)?;
    let reg_data: registry::RegistryData =
        serde_json::from_slice(&json_bytes).map_err(|e| BackupFileError::Unexpected(e.into()))?;

    match registry::import_registry_data(&reg_data) {
        Ok(()) => Ok(RestoreOutcome::Restored),
        Err(registry::RegistryError::UnsupportedPlatform) => {
            Ok(RestoreOutcome::Skipped(SkipReason::UnsupportedPlatform))
        }
        Err(e) => Err(BackupFileError::RegistryError(e.to_string())),
    }
}

fn find_v2_restore_source(
    temp_root: &Path,
    save_unit_id: u32,
    expected_name: &std::ffi::OsStr,
) -> Result<Option<PathBuf>, BackupFileError> {
    let save_unit_root = temp_root.join(save_unit_id.to_string());
    if !save_unit_root.exists() {
        return Ok(None);
    }

    let mut entries = fs::read_dir(&save_unit_root)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, std::io::Error>>()?;
    entries.sort();

    if entries.is_empty() {
        return Ok(None);
    }

    if let Some(matched) = entries
        .iter()
        .find(|entry| entry.file_name().is_some_and(|name| name == expected_name))
    {
        return Ok(Some(matched.clone()));
    }

    Ok(entries.into_iter().next())
}

fn restore_save_unit_from_temp(
    unit: &SaveUnit,
    version: ArchiveVersion,
    temp_root: &Path,
    notifier: Option<&dyn RestoreNotifier>,
) -> Result<RestoreOutcome, BackupFileError> {
    if let SaveUnitType::WinRegistry = unit.unit_type {
        return restore_registry_unit(unit, version, temp_root);
    }

    let unit_path = unit.resolve_path_for_current_device()?;
    let file_name = unit_path
        .file_name()
        .ok_or(BackupFileError::NonePathError)?;

    // V2+ archives store entries under `{save_unit_id}/{name}`, older versions use flat layout.
    // The ID is a stable, monotonically-assigned identifier that does not change when
    // save units are added or removed.
    let original_path = if version.uses_save_unit_prefix() {
        find_v2_restore_source(temp_root, unit.id, file_name)?
    } else {
        let legacy_path = temp_root.join(file_name);
        legacy_path.exists().then_some(legacy_path)
    };

    let Some(original_path) = original_path else {
        return Ok(RestoreOutcome::Skipped(SkipReason::MissingArchiveEntry));
    };

    match unit.unit_type {
        SaveUnitType::File => {
            restore_file_unit(unit, original_path, unit_path, notifier)?;
            Ok(RestoreOutcome::Restored)
        }
        SaveUnitType::Folder => {
            restore_folder_unit(unit, original_path, unit_path, notifier)?;
            Ok(RestoreOutcome::Restored)
        }
        SaveUnitType::WinRegistry => unreachable!(),
    }
}

/// Decompress a zip archive at the given path to the original save-unit paths.
pub(super) fn decompress_from_archive(
    save_paths: &[SaveUnit],
    archive_path: &Path,
    notifier: Option<&dyn RestoreNotifier>,
) -> Result<(), CompressError> {
    let file = File::open(archive_path).map_err(|e| CompressError::Single(e.into()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| CompressError::Single(e.into()))?;

    let version = ArchiveVersion::from_comment(zip.comment());

    let temp_dir = temp_dir::TempDir::new().map_err(|e| CompressError::Single(e.into()))?;
    let temp_root = temp_dir.path().to_path_buf();
    fs::create_dir_all(&temp_root).map_err(|e| CompressError::Single(e.into()))?;

    let mut dir_timestamps = extract_zip_entries_to_temp(&mut zip, &temp_root, version)?;
    dir_timestamps.sort_by(|a, b| {
        let depth_a = a.0.components().count();
        let depth_b = b.0.components().count();
        depth_b.cmp(&depth_a)
    });

    for (dir_path, file_time) in dir_timestamps {
        let _ = set_file_mtime(&dir_path, file_time);
    }

    let mut restore_errors = Vec::new();
    for unit in save_paths.iter().filter(|unit| unit.enabled) {
        match restore_save_unit_from_temp(unit, version, &temp_root, notifier) {
            Ok(RestoreOutcome::Restored) => {}
            Ok(RestoreOutcome::Skipped(reason)) => {
                emit_skip_notification(unit, &reason, notifier);
            }
            Err(err) => restore_errors.push(err),
        }
    }

    if !restore_errors.is_empty() {
        return Err(CompressError::Multiple(restore_errors));
    }

    Ok(())
}

/// Decompress a zip file to the original save-unit paths.
///
/// Constructs the archive path from `backup_path` and `date` (e.g., `backup_path/date.zip`).
#[cfg(test)]
pub fn decompress_from_file(
    save_paths: &[SaveUnit],
    backup_path: &Path,
    date: &str,
    notifier: Option<&dyn RestoreNotifier>,
) -> Result<(), CompressError> {
    let zip_path = backup_path.join([date, ".zip"].concat());
    decompress_from_archive(save_paths, &zip_path, notifier)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::save_unit_label;
    use crate::backup::{SaveUnit, SaveUnitType};

    #[test]
    fn save_unit_label_prefers_sorted_device_fallback() {
        let mut paths = HashMap::new();
        paths.insert("z-device".to_string(), "Z:\\save".to_string());
        paths.insert("a-device".to_string(), "A:\\save".to_string());

        let unit = SaveUnit {
            id: 7,
            unit_type: SaveUnitType::Folder,
            paths,
            delete_before_apply: false,
            enabled: true,
        };

        assert_eq!(save_unit_label(&unit), "A:\\save");
    }

    #[test]
    fn save_unit_label_falls_back_to_id_when_no_paths_exist() {
        let unit = SaveUnit {
            id: 42,
            unit_type: SaveUnitType::File,
            paths: HashMap::new(),
            delete_before_apply: false,
            enabled: true,
        };

        assert_eq!(save_unit_label(&unit), "#42");
    }
}
