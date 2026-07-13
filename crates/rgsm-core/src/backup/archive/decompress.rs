//! ZIP archive extraction and save unit restoration.
//!
//! Handles all archive versions (Legacy, V1, V2) transparently.
//! V2 archives have index-prefixed entries that are mapped back to save units by stable ID.

use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use filetime::{FileTime, set_file_mtime};
use fs_extra::{dir::move_dir, file::move_file};
use log::warn;
use rust_i18n::t;

use crate::{
    backup::{CaptureSourceKind, RestorePlan, SaveUnit, SaveUnitType},
    device::get_current_device_id,
    path_resolver::PathContext,
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

use super::{
    ArchiveManifestV3, V3_MANIFEST_ENTRY, timestamp::zip_datetime_to_system_time,
    version::ArchiveVersion,
};

pub(super) fn archive_version(archive_path: &Path) -> Result<ArchiveVersion, CompressError> {
    let file = File::open(archive_path).map_err(|error| CompressError::Single(error.into()))?;
    let zip = zip::ZipArchive::new(file).map_err(|error| CompressError::Single(error.into()))?;
    Ok(ArchiveVersion::from_comment(zip.comment()))
}

pub(super) fn read_capture_manifest(
    archive_path: &Path,
) -> Result<ArchiveManifestV3, CompressError> {
    let file = File::open(archive_path).map_err(|error| CompressError::Single(error.into()))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|error| CompressError::Single(error.into()))?;
    if ArchiveVersion::from_comment(zip.comment()) != ArchiveVersion::V3 {
        return Err(CompressError::Single(BackupFileError::Unexpected(
            anyhow::anyhow!("capture manifest requested from a non-V3 archive"),
        )));
    }
    serde_json::from_reader(
        zip.by_name(V3_MANIFEST_ENTRY)
            .map_err(|error| CompressError::Single(error.into()))?,
    )
    .map_err(|error| CompressError::Single(BackupFileError::Unexpected(error.into())))
}

pub(super) fn restore_capture_plan(
    plan: &RestorePlan,
    archive_path: &Path,
) -> Result<(), CompressError> {
    let file = File::open(archive_path).map_err(|error| CompressError::Single(error.into()))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|error| CompressError::Single(error.into()))?;
    verify_capture_entries(&mut zip, plan)?;
    for entry in &plan.entries {
        if entry.delete_before_apply && entry.target_path.exists() {
            let result = if entry.target_path.is_dir() {
                fs::remove_dir_all(&entry.target_path)
            } else {
                fs::remove_file(&entry.target_path)
            };
            result.map_err(|error| CompressError::Single(error.into()))?;
        }
        match entry.kind {
            CaptureSourceKind::Registry => restore_registry_capture(&mut zip, entry)?,
            CaptureSourceKind::File => restore_file_capture(&mut zip, entry)?,
            CaptureSourceKind::Directory => restore_directory_capture(&mut zip, entry)?,
        }
    }
    Ok(())
}

fn verify_capture_entries(
    zip: &mut zip::ZipArchive<File>,
    plan: &RestorePlan,
) -> Result<(), CompressError> {
    for entry in &plan.entries {
        match entry.kind {
            CaptureSourceKind::File | CaptureSourceKind::Registry => {
                zip.by_name(&entry.archive_path)
                    .map_err(|error| CompressError::Single(error.into()))?;
            }
            CaptureSourceKind::Directory => {
                let prefix = format!("{}/", entry.archive_path.trim_end_matches('/'));
                if !zip.file_names().any(|name| name.starts_with(&prefix)) {
                    return Err(CompressError::Single(BackupFileError::Unexpected(
                        anyhow::anyhow!("capture directory is missing from archive: {prefix}"),
                    )));
                }
            }
        }
    }
    Ok(())
}

fn restore_file_capture(
    zip: &mut zip::ZipArchive<File>,
    entry: &crate::backup::RestoreEntry,
) -> Result<(), CompressError> {
    let mut source = zip
        .by_name(&entry.archive_path)
        .map_err(|error| CompressError::Single(error.into()))?;
    if let Some(parent) = entry.target_path.parent() {
        fs::create_dir_all(parent).map_err(|error| CompressError::Single(error.into()))?;
    }
    let mut target =
        File::create(&entry.target_path).map_err(|error| CompressError::Single(error.into()))?;
    std::io::copy(&mut source, &mut target).map_err(|error| CompressError::Single(error.into()))?;
    if let Some(zip_time) = source.last_modified() {
        let _ = set_file_mtime(
            &entry.target_path,
            FileTime::from_system_time(zip_datetime_to_system_time(zip_time, ArchiveVersion::V3)),
        );
    }
    Ok(())
}

fn restore_directory_capture(
    zip: &mut zip::ZipArchive<File>,
    entry: &crate::backup::RestoreEntry,
) -> Result<(), CompressError> {
    let prefix = format!("{}/", entry.archive_path.trim_end_matches('/'));
    let mut directory_times = Vec::new();
    for index in 0..zip.len() {
        let mut source = zip
            .by_index(index)
            .map_err(|error| CompressError::Single(error.into()))?;
        let Some(relative) = source.name().strip_prefix(&prefix) else {
            continue;
        };
        let relative = PathBuf::from(relative);
        if relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        }) {
            continue;
        }
        let target = entry.target_path.join(&relative);
        if source.is_dir() {
            fs::create_dir_all(&target).map_err(|error| CompressError::Single(error.into()))?;
            if let Some(zip_time) = source.last_modified() {
                directory_times.push((
                    target,
                    FileTime::from_system_time(zip_datetime_to_system_time(
                        zip_time,
                        ArchiveVersion::V3,
                    )),
                ));
            }
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| CompressError::Single(error.into()))?;
        }
        let mut output =
            File::create(target).map_err(|error| CompressError::Single(error.into()))?;
        std::io::copy(&mut source, &mut output)
            .map_err(|error| CompressError::Single(error.into()))?;
        if let Some(zip_time) = source.last_modified() {
            let _ = set_file_mtime(
                entry.target_path.join(relative),
                FileTime::from_system_time(zip_datetime_to_system_time(
                    zip_time,
                    ArchiveVersion::V3,
                )),
            );
        }
    }
    directory_times.sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
    for (path, time) in directory_times {
        set_file_mtime(path, time).map_err(|error| CompressError::Single(error.into()))?;
    }
    Ok(())
}

fn restore_registry_capture(
    zip: &mut zip::ZipArchive<File>,
    entry: &crate::backup::RestoreEntry,
) -> Result<(), CompressError> {
    let mut source = zip
        .by_name(&entry.archive_path)
        .map_err(|error| CompressError::Single(error.into()))?;
    let mut bytes = Vec::new();
    source
        .read_to_end(&mut bytes)
        .map_err(|error| CompressError::Single(error.into()))?;
    let data = crate::backup::registry::deserialize_reg_file(&bytes).map_err(|error| {
        CompressError::Single(BackupFileError::RegistryError(error.to_string()))
    })?;
    crate::backup::registry::import_registry_data(&data)
        .map_err(|error| CompressError::Single(BackupFileError::RegistryError(error.to_string())))
}

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
    let Some(paths) = unit.paths() else {
        return format!("#{}", unit.id);
    };
    if let Some(path) = paths.get(get_current_device_id()) {
        return path.clone();
    }

    let mut entries: Vec<_> = paths.iter().collect();
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
/// Reads `{id}/registry.reg` (or legacy `{id}/registry.json`), parses it, and
/// imports the values back into the Windows Registry. On non-Windows platforms
/// the restore is skipped.
fn restore_registry_unit(
    unit: &SaveUnit,
    version: ArchiveVersion,
    temp_root: &Path,
) -> Result<RestoreOutcome, BackupFileError> {
    use crate::backup::registry;

    if !version.uses_save_unit_prefix() {
        return Ok(RestoreOutcome::Skipped(SkipReason::LegacyArchiveFormat));
    }

    let reg_unit_root = temp_root.join(unit.id.to_string());
    let reg_path = reg_unit_root.join(registry::REGISTRY_DATA_FILENAME);
    let legacy_json_path = reg_unit_root.join(registry::LEGACY_REGISTRY_DATA_FILENAME);

    if !reg_path.exists() && !legacy_json_path.exists() {
        return Ok(RestoreOutcome::Skipped(SkipReason::MissingArchiveEntry));
    }

    let reg_data = if reg_path.exists() {
        let reg_bytes = fs::read(&reg_path)?;
        registry::deserialize_reg_file(&reg_bytes)
            .map_err(|e| BackupFileError::RegistryError(e.to_string()))?
    } else {
        let json_bytes = fs::read(&legacy_json_path)?;
        serde_json::from_slice::<registry::RegistryData>(&json_bytes)
            .map_err(|e| BackupFileError::Unexpected(e.into()))?
    };

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
    path_ctx: Option<&PathContext>,
) -> Result<RestoreOutcome, BackupFileError> {
    if matches!(unit.unit_type(), Some(SaveUnitType::WinRegistry)) {
        return restore_registry_unit(unit, version, temp_root);
    }

    let unit_path = unit.resolve_path_for_current_device(path_ctx)?;
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

    match unit.unit_type().ok_or(BackupFileError::NonePathError)? {
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
    path_ctx: Option<&PathContext>,
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
        match restore_save_unit_from_temp(unit, version, &temp_root, notifier, path_ctx) {
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
    decompress_from_archive(save_paths, &zip_path, notifier, None)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use filetime::{FileTime, set_file_mtime};

    use super::save_unit_label;
    use crate::backup::{SaveUnit, SaveUnitType};

    #[test]
    fn v3_restore_plan_extracts_only_approved_target() {
        use crate::backup::{
            ArchiveBackend, CaptureGroup, CapturePlan, CaptureSourceKind, CompressionPreset,
            RestoreEntry, RestorePlan, ZipBackend,
        };
        use crate::path_resolution::CandidateDimensions;

        let temp = temp_dir::TempDir::new().unwrap();
        let source = temp.path().join("source.dat");
        let target = temp.path().join("restore").join("target.dat");
        let archive = temp.path().join("snapshot.zip");
        fs::write(&source, b"captured").unwrap();
        let capture = CapturePlan {
            groups: vec![CaptureGroup {
                id: 0,
                save_unit_id: 4,
                candidate_id: "source".to_string(),
                dimensions: CandidateDimensions::default(),
                logical_anchor: temp.path().to_path_buf(),
                source_path: source.to_string_lossy().into_owned(),
                relative_path: "source.dat".to_string(),
                archive_path: "4/0/data/source.dat".to_string(),
                kind: CaptureSourceKind::File,
                delete_before_apply: true,
            }],
        };
        ZipBackend
            .compress_capture_plan(&capture, &archive, CompressionPreset::Standard, None)
            .unwrap();
        let restore = RestorePlan {
            entries: vec![RestoreEntry {
                save_unit_id: 4,
                group_id: 0,
                archive_path: "4/0/data/source.dat".to_string(),
                target_path: target.clone(),
                kind: CaptureSourceKind::File,
                delete_before_apply: true,
            }],
        };

        ZipBackend.restore_capture_plan(&restore, &archive).unwrap();

        assert_eq!(fs::read(target).unwrap(), b"captured");
        assert_eq!(fs::read(source).unwrap(), b"captured");
    }

    #[test]
    fn v3_restore_verifies_entries_before_deleting_existing_target() {
        use crate::backup::{CaptureSourceKind, RestoreEntry, RestorePlan};

        let temp = temp_dir::TempDir::new().unwrap();
        let archive = temp.path().join("empty.zip");
        let target = temp.path().join("save.dat");
        fs::write(&target, b"keep").unwrap();
        zip::ZipWriter::new(fs::File::create(&archive).unwrap())
            .finish()
            .unwrap();
        let restore = RestorePlan {
            entries: vec![RestoreEntry {
                save_unit_id: 1,
                group_id: 0,
                archive_path: "1/0/data/save.dat".to_string(),
                target_path: target.clone(),
                kind: CaptureSourceKind::File,
                delete_before_apply: true,
            }],
        };

        assert!(super::restore_capture_plan(&restore, &archive).is_err());
        assert_eq!(fs::read(target).unwrap(), b"keep");
    }

    #[test]
    fn v3_restore_reapplies_nested_directory_mtimes_after_children() {
        use crate::backup::{
            ArchiveBackend, CaptureGroup, CapturePlan, CaptureSourceKind, CompressionPreset,
            RestoreEntry, RestorePlan, ZipBackend,
        };
        use crate::path_resolution::CandidateDimensions;

        let temp = temp_dir::TempDir::new().unwrap();
        let source = temp.path().join("source");
        let nested = source.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("save.dat"), b"save").unwrap();
        let source_time = FileTime::from_unix_time(1_704_164_644, 0);
        let nested_time = FileTime::from_unix_time(1_704_251_044, 0);
        set_file_mtime(&nested, nested_time).unwrap();
        set_file_mtime(&source, source_time).unwrap();
        let archive = temp.path().join("snapshot.zip");
        let capture = CapturePlan {
            groups: vec![CaptureGroup {
                id: 0,
                save_unit_id: 1,
                candidate_id: "source".into(),
                dimensions: CandidateDimensions::default(),
                logical_anchor: temp.path().to_path_buf(),
                source_path: source.to_string_lossy().into_owned(),
                relative_path: "source".into(),
                archive_path: "1/0/data/source".into(),
                kind: CaptureSourceKind::Directory,
                delete_before_apply: false,
            }],
        };
        ZipBackend
            .compress_capture_plan(&capture, &archive, CompressionPreset::Fast, None)
            .unwrap();
        let target = temp.path().join("restored");
        ZipBackend
            .restore_capture_plan(
                &RestorePlan {
                    entries: vec![RestoreEntry {
                        save_unit_id: 1,
                        group_id: 0,
                        archive_path: "1/0/data/source".into(),
                        target_path: target.clone(),
                        kind: CaptureSourceKind::Directory,
                        delete_before_apply: false,
                    }],
                },
                &archive,
            )
            .unwrap();

        assert_eq!(
            FileTime::from_last_modification_time(&fs::metadata(&target).unwrap()),
            source_time
        );
        assert_eq!(
            FileTime::from_last_modification_time(&fs::metadata(target.join("nested")).unwrap()),
            nested_time
        );
    }

    #[test]
    fn save_unit_label_prefers_sorted_device_fallback() {
        let mut paths = HashMap::new();
        paths.insert("z-device".to_string(), "Z:\\save".to_string());
        paths.insert("a-device".to_string(), "A:\\save".to_string());

        let unit = SaveUnit::concrete(7, SaveUnitType::Folder, paths, false, true);

        assert_eq!(save_unit_label(&unit), "A:\\save");
    }

    #[test]
    fn save_unit_label_falls_back_to_id_when_no_paths_exist() {
        let unit = SaveUnit {
            id: 42,
            source: crate::backup::SaveUnitSource::Concrete {
                unit_type: SaveUnitType::File,
                paths: HashMap::new(),
            },
            delete_before_apply: false,
            enabled: true,
        };

        assert_eq!(save_unit_label(&unit), "#42");
    }
}
