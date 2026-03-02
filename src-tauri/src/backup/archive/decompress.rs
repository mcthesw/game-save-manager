//! ZIP archive extraction and save unit restoration.
//!
//! Handles all archive versions (Legacy, V1, V2) transparently.
//! V2 archives have index-prefixed entries that are mapped back to save units by index.

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

use filetime::{FileTime, set_file_mtime};
use fs_extra::{dir::move_dir, file::move_file};
use log::warn;
use rust_i18n::t;
use tauri::{AppHandle, Emitter};

use crate::{
    backup::{SaveUnit, SaveUnitType},
    ipc_handler::{IpcNotification, NotificationLevel},
    preclude::*,
};

use super::{timestamp::zip_datetime_to_system_time, version::ArchiveVersion};

fn emit_missing_path_warning(
    path: &Path,
    app_handle: Option<&AppHandle>,
) -> Result<(), BackupFileError> {
    warn!(
        target:"rgsm::backup::archive",
        "Path {:#?} not exists, auto created",
        path.to_str().unwrap_or("path.to_str error")
    );

    if let Some(app_handle) = app_handle {
        let msg_path = path.to_str().unwrap_or("path.to_str error");
        app_handle
            .emit(
                "Notification",
                IpcNotification {
                    level: NotificationLevel::warning,
                    title: "WARNING".to_string(),
                    msg: t!("backend.archive.file_not_exist", path = msg_path).to_string(),
                },
            )
            .map_err(anyhow::Error::from)?;
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
    app_handle: Option<&AppHandle>,
) -> Result<(), BackupFileError> {
    let parent = target_path.parent().ok_or(BackupFileError::NonePathError)?;
    let restored_file_mtime = fs::metadata(&original_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(FileTime::from_system_time);

    if !parent.exists() {
        emit_missing_path_warning(parent, app_handle)?;
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
    app_handle: Option<&AppHandle>,
) -> Result<(), BackupFileError> {
    let parent = target_path.parent().ok_or(BackupFileError::NonePathError)?;

    if !parent.exists() {
        emit_missing_path_warning(parent, app_handle)?;
        fs::create_dir_all(parent)?;
    }

    if unit.delete_before_apply && target_path.exists() {
        fs::remove_dir_all(&target_path)?;
    }

    let option = fs_extra::dir::CopyOptions::new().overwrite(true);
    move_dir(original_path, parent, &option)?;

    Ok(())
}

/// Restore a Windows Registry save unit from the extracted temp directory.
///
/// Reads `{id}/registry.json`, parses it, and imports the values back into
/// the Windows Registry. On non-Windows platforms the import is silently skipped.
fn restore_registry_unit(
    unit: &SaveUnit,
    version: ArchiveVersion,
    temp_root: &Path,
) -> Result<(), BackupFileError> {
    use crate::backup::registry;

    if !version.uses_index_prefix() {
        warn!(target: "rgsm::backup::archive", "Registry restore skipped: legacy archive format");
        return Ok(());
    }

    let reg_json_path = temp_root
        .join(unit.id.to_string())
        .join(registry::REGISTRY_DATA_FILENAME);

    if !reg_json_path.exists() {
        warn!(target: "rgsm::backup::archive", "Registry data file not found: {}", reg_json_path.display());
        return Ok(());
    }

    let json_bytes = fs::read(&reg_json_path)?;
    let reg_data: registry::RegistryData =
        serde_json::from_slice(&json_bytes).map_err(|e| BackupFileError::Unexpected(e.into()))?;

    match registry::import_registry_data(&reg_data) {
        Ok(()) => Ok(()),
        Err(registry::RegistryError::UnsupportedPlatform) => {
            warn!(target: "rgsm::backup::archive", "Registry restore skipped: not on Windows");
            Ok(())
        }
        Err(e) => Err(BackupFileError::RegistryError(e.to_string())),
    }
}

fn restore_save_unit_from_temp(
    unit: &SaveUnit,
    version: ArchiveVersion,
    temp_root: &Path,
    app_handle: Option<&AppHandle>,
) -> Result<(), BackupFileError> {
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
    let original_path = if version.uses_index_prefix() {
        temp_root.join(unit.id.to_string()).join(file_name)
    } else {
        temp_root.join(file_name)
    };

    if !original_path.exists() {
        return Err(BackupFileError::NotExists(original_path));
    }

    match unit.unit_type {
        SaveUnitType::File => restore_file_unit(unit, original_path, unit_path, app_handle),
        SaveUnitType::Folder => restore_folder_unit(unit, original_path, unit_path, app_handle),
        SaveUnitType::WinRegistry => unreachable!(),
    }
}

/// Decompress a zip archive at the given path to the original save-unit paths.
pub(super) fn decompress_from_archive(
    save_paths: &[SaveUnit],
    archive_path: &Path,
    app_handle: Option<&AppHandle>,
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
    for unit in save_paths {
        if let Err(err) = restore_save_unit_from_temp(unit, version, &temp_root, app_handle) {
            restore_errors.push(err);
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
    app_handle: Option<&AppHandle>,
) -> Result<(), CompressError> {
    let zip_path = backup_path.join([date, ".zip"].concat());
    decompress_from_archive(save_paths, &zip_path, app_handle)
}
