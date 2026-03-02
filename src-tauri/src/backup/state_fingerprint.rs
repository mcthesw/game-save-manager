use std::{
    fs::{self, File},
    hash::Hasher,
    path::{Path, PathBuf},
    time::SystemTime,
};

use xxhash_rust::xxh3::Xxh3;

use crate::backup::path_format::path_to_zip_style;
#[cfg(target_os = "windows")]
use crate::backup::registry;
#[cfg(target_os = "windows")]
use crate::device::get_current_device_id;
use crate::{
    backup::{SaveUnit, SaveUnitType},
    preclude::*,
};

use super::archive::{ArchiveMeta, ArchiveVersion, system_time_to_zip_datetime};

const FINGERPRINT_MAGIC: &[u8] = b"RGSM_FP_V1";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum EntryKind {
    Dir = 0,
    File = 1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SaveEntryMeta {
    rel_path: String,
    kind: EntryKind,
    size: u64,
    mtime_zip: zip::DateTime,
}

fn metadata_mtime_to_zip(path: &Path) -> Result<zip::DateTime, BackupFileError> {
    let modified = fs::metadata(path)?
        .modified()
        .unwrap_or_else(|_| SystemTime::now());
    Ok(system_time_to_zip_datetime(modified))
}

fn collect_file_entry(path: &Path, rel_path: &Path) -> Result<SaveEntryMeta, BackupFileError> {
    let size = fs::metadata(path)?.len();
    let rel_path = path_to_zip_style(rel_path)?;
    let mtime_zip = metadata_mtime_to_zip(path)?;
    Ok(SaveEntryMeta {
        rel_path,
        kind: EntryKind::File,
        size,
        mtime_zip,
    })
}

fn collect_dir_entry(path: &Path, rel_path: &Path) -> Result<SaveEntryMeta, BackupFileError> {
    let rel_path = path_to_zip_style(rel_path)?;
    let mtime_zip = metadata_mtime_to_zip(path)?;
    Ok(SaveEntryMeta {
        rel_path,
        kind: EntryKind::Dir,
        size: 0,
        mtime_zip,
    })
}

fn collect_directory_entries(
    source_dir: &Path,
    rel_root: &Path,
    out: &mut Vec<SaveEntryMeta>,
) -> Result<(), BackupFileError> {
    out.push(collect_dir_entry(source_dir, rel_root)?);

    for entry in fs::read_dir(source_dir)? {
        let entry = entry?;
        let entry_path = entry.path();
        let entry_rel = rel_root.join(entry.file_name());

        if entry_path.is_file() {
            out.push(collect_file_entry(&entry_path, &entry_rel)?);
        } else if entry_path.is_dir() {
            collect_directory_entries(&entry_path, &entry_rel, out)?;
        }
    }

    Ok(())
}

fn sort_entries(entries: &mut [SaveEntryMeta]) {
    entries.sort_by(|left, right| {
        left.rel_path
            .cmp(&right.rel_path)
            .then_with(|| left.kind.cmp(&right.kind))
    });
}

fn update_hasher_with_entry(hasher: &mut Xxh3, entry: &SaveEntryMeta) {
    let rel_path_bytes = entry.rel_path.as_bytes();
    hasher.write(&(rel_path_bytes.len() as u32).to_le_bytes());
    hasher.write(rel_path_bytes);
    hasher.write(&[entry.kind as u8]);
    hasher.write(&entry.size.to_le_bytes());

    hasher.write(&entry.mtime_zip.year().to_le_bytes());
    hasher.write(&[entry.mtime_zip.month()]);
    hasher.write(&[entry.mtime_zip.day()]);
    hasher.write(&[entry.mtime_zip.hour()]);
    hasher.write(&[entry.mtime_zip.minute()]);
    hasher.write(&[entry.mtime_zip.second()]);
}

fn build_fingerprint(mut entries: Vec<SaveEntryMeta>) -> String {
    sort_entries(&mut entries);

    let mut hasher = Xxh3::new();
    hasher.write(FINGERPRINT_MAGIC);

    for entry in &entries {
        update_hasher_with_entry(&mut hasher, entry);
    }

    format!("{:016x}", hasher.finish())
}

fn collect_entries_from_source(
    save_paths: &[SaveUnit],
) -> Result<Vec<SaveEntryMeta>, BackupFileError> {
    let mut entries = Vec::new();

    for save_unit in save_paths {
        if let SaveUnitType::WinRegistry = save_unit.unit_type {
            // Registry content is fingerprinted separately via extend_with_registry().
            continue;
        }

        let source_path = save_unit.resolve_path_for_current_device()?;
        if !source_path.exists() {
            return Err(BackupFileError::NotExists(source_path));
        }

        match save_unit.unit_type {
            SaveUnitType::File => {
                let file_name = source_path
                    .file_name()
                    .ok_or(BackupFileError::NonePathError)?;
                entries.push(collect_file_entry(&source_path, &PathBuf::from(file_name))?);
            }
            SaveUnitType::Folder => {
                let folder_name = source_path
                    .file_name()
                    .ok_or(BackupFileError::NonePathError)?;
                collect_directory_entries(&source_path, &PathBuf::from(folder_name), &mut entries)?;
            }
            SaveUnitType::WinRegistry => unreachable!(),
        }
    }

    Ok(entries)
}

/// Extend a base fingerprint by hashing Windows Registry content for any
/// `WinRegistry` save units. For file-only games the base is returned unchanged,
/// keeping backward compatibility with older archives.
#[cfg(target_os = "windows")]
fn extend_with_registry(base: String, save_paths: &[SaveUnit]) -> Result<String, CompressError> {
    let mut registry_data = Vec::new();
    for save_unit in save_paths {
        if let SaveUnitType::WinRegistry = save_unit.unit_type {
            let reg_path = save_unit
                .get_path_for_device(get_current_device_id())
                .ok_or(CompressError::Single(BackupFileError::NonePathError))?;
            let reg = registry::export_registry_key(reg_path).map_err(|e| {
                CompressError::Single(BackupFileError::RegistryError(e.to_string()))
            })?;
            let json = serde_json::to_vec_pretty(&reg)
                .map_err(|e| CompressError::Single(BackupFileError::Unexpected(e.into())))?;
            registry_data.push(json);
        }
    }

    if registry_data.is_empty() {
        return Ok(base);
    }

    let mut hasher = Xxh3::new();
    hasher.write(base.as_bytes());
    hasher.write(b"RGSM_REG");
    for data in &registry_data {
        hasher.write(&(data.len() as u64).to_le_bytes());
        hasher.write(data);
    }
    Ok(format!("{:016x}", hasher.finish()))
}

#[cfg(not(target_os = "windows"))]
fn extend_with_registry(base: String, _save_paths: &[SaveUnit]) -> Result<String, CompressError> {
    Ok(base)
}

pub(crate) fn fingerprint_source_state(save_paths: &[SaveUnit]) -> Result<String, CompressError> {
    let entries = collect_entries_from_source(save_paths).map_err(CompressError::Single)?;
    let base = build_fingerprint(entries);
    extend_with_registry(base, save_paths)
}

/// Read the stored source fingerprint from a ZIP archive's comment metadata.
pub(crate) fn read_stored_fingerprint(zip_path: &Path) -> Option<String> {
    let file = File::open(zip_path).ok()?;
    let archive = zip::ZipArchive::new(file).ok()?;
    let meta = ArchiveMeta::from_comment(archive.comment())?;
    meta.source_fingerprint
}

fn normalize_zip_entry_name(name: &str, is_dir: bool) -> String {
    let normalized = name.replace('\\', "/");
    if is_dir {
        normalized.trim_end_matches('/').to_string()
    } else {
        normalized
    }
}

fn collect_entries_from_zip(
    archive: &mut zip::ZipArchive<File>,
    version: ArchiveVersion,
) -> Result<Vec<SaveEntryMeta>, CompressError> {
    let mut entries = Vec::new();

    for index in 0..archive.len() {
        let zip_file = archive
            .by_index(index)
            .map_err(|e| CompressError::Single(e.into()))?;
        let is_dir = zip_file.is_dir();
        let raw_path = normalize_zip_entry_name(zip_file.name(), is_dir);

        let rel_path = match version.normalize_entry_path(&raw_path) {
            Some(p) => p.to_string(),
            None => continue,
        };

        if rel_path.is_empty() {
            continue;
        }

        entries.push(SaveEntryMeta {
            rel_path,
            kind: if is_dir {
                EntryKind::Dir
            } else {
                EntryKind::File
            },
            size: if is_dir { 0 } else { zip_file.size() },
            mtime_zip: zip_file.last_modified().unwrap_or_default(),
        });
    }

    Ok(entries)
}

pub(crate) fn fingerprint_zip_state(zip_path: &Path) -> Result<Option<String>, CompressError> {
    let file = File::open(zip_path).map_err(|e| CompressError::Single(e.into()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| CompressError::Single(e.into()))?;

    let version = ArchiveVersion::from_comment(archive.comment());
    if !version.uses_local_timestamps() {
        return Ok(None);
    }

    let entries = collect_entries_from_zip(&mut archive, version)?;
    Ok(Some(build_fingerprint(entries)))
}

/// Compute an XXH3 hash of a file's contents for integrity verification.
pub(crate) fn compute_file_hash(path: &Path) -> Result<String, CompressError> {
    let data = fs::read(path).map_err(|e| CompressError::Single(e.into()))?;
    let mut hasher = Xxh3::new();
    hasher.write(&data);
    Ok(format!("{:016x}", hasher.finish()))
}
