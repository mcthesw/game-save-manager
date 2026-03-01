use std::{
    fs::{self, File},
    hash::Hasher,
    path::{Path, PathBuf},
    time::SystemTime,
};

use xxhash_rust::xxh3::Xxh3;

use crate::backup::path_format::path_to_zip_style;
use crate::{
    backup::{SaveUnit, SaveUnitType},
    preclude::*,
};

use super::archive::{ArchiveVersion, system_time_to_zip_datetime};

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
        }
    }

    Ok(entries)
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

pub(crate) fn fingerprint_source_state(save_paths: &[SaveUnit]) -> Result<String, CompressError> {
    let entries = collect_entries_from_source(save_paths).map_err(CompressError::Single)?;
    Ok(build_fingerprint(entries))
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
