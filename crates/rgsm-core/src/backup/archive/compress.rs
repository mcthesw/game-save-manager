//! ZIP archive creation from save units.
//!
//! Writes V2 archives with ID-prefixed entries and structured metadata.
//! Each save unit is stored under `{id}/` to prevent same-name collisions.

use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use zip::{ZipWriter, write::SimpleFileOptions};

use crate::backup::path_format::path_to_zip_style;
use crate::{
    backup::{CompressionPreset, SaveUnit, SaveUnitType, registry},
    device::get_current_device_id,
    path_resolver::PathContext,
    preclude::*,
};

use super::version::ArchiveMeta;

use super::timestamp::system_time_to_zip_datetime;

fn file_modified_to_zip_datetime(path: &Path) -> Result<zip::DateTime, BackupFileError> {
    let modified = fs::metadata(path)?
        .modified()
        .unwrap_or_else(|_| SystemTime::now());
    Ok(system_time_to_zip_datetime(modified))
}

fn zip_options_with_mtime(
    path: &Path,
    preset: CompressionPreset,
) -> Result<SimpleFileOptions, BackupFileError> {
    let mtime = file_modified_to_zip_datetime(path)?;
    let mut opts = SimpleFileOptions::default()
        .compression_method(preset.zip_method())
        .last_modified_time(mtime);
    if let Some(level) = preset.compression_level() {
        opts = opts.compression_level(Some(level));
    }
    Ok(opts)
}

fn write_file_entry<T>(
    writer: &mut ZipWriter<T>,
    source_path: &Path,
    entry_name: &Path,
    preset: CompressionPreset,
) -> Result<(), BackupFileError>
where
    T: Write + Seek,
{
    let mut file = File::open(source_path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    writer.start_file(
        path_to_zip_style(entry_name)?,
        zip_options_with_mtime(source_path, preset)?,
    )?;
    writer.write_all(&buffer)?;
    Ok(())
}

fn write_directory_entry<T>(
    writer: &mut ZipWriter<T>,
    source_path: &Path,
    entry_name: &Path,
    preset: CompressionPreset,
) -> Result<(), BackupFileError>
where
    T: Write + Seek,
{
    writer.add_directory(
        path_to_zip_style(entry_name)?,
        zip_options_with_mtime(source_path, preset)?,
    )?;
    Ok(())
}

/// Write raw bytes as a ZIP entry (not backed by a filesystem file).
fn write_bytes_entry<T>(
    writer: &mut ZipWriter<T>,
    data: &[u8],
    entry_name: &Path,
    preset: CompressionPreset,
) -> Result<(), BackupFileError>
where
    T: Write + Seek,
{
    let mtime = system_time_to_zip_datetime(SystemTime::now());
    let mut opts = SimpleFileOptions::default()
        .compression_method(preset.zip_method())
        .last_modified_time(mtime);
    if let Some(level) = preset.compression_level() {
        opts = opts.compression_level(Some(level));
    }
    writer.start_file(path_to_zip_style(entry_name)?, opts)?;
    writer.write_all(data)?;
    Ok(())
}

/// Append a Windows Registry save unit: export the key tree to a standard
/// `.reg` file and store it as `{id}/registry.reg` inside the archive.
fn append_registry_unit<T>(
    writer: &mut ZipWriter<T>,
    save_unit: &SaveUnit,
    id_prefix: &Path,
    preset: CompressionPreset,
) -> Result<(), BackupFileError>
where
    T: Write + Seek,
{
    let reg_path_str = save_unit
        .get_path_for_device(get_current_device_id())
        .ok_or(BackupFileError::NonePathError)?;
    let reg_data = match registry::export_registry_key(reg_path_str) {
        Ok(data) => data,
        Err(registry::RegistryError::UnsupportedPlatform) => {
            log::warn!(target: "rgsm::backup", "Skipping registry save unit on unsupported platform");
            return Ok(());
        }
        Err(e) => return Err(BackupFileError::RegistryError(e.to_string())),
    };
    let reg_bytes = registry::serialize_reg_file(&reg_data)
        .map_err(|e| BackupFileError::RegistryError(e.to_string()))?;
    write_bytes_entry(
        writer,
        &reg_bytes,
        &id_prefix.join(registry::REGISTRY_DATA_FILENAME),
        preset,
    )
}

/// Write a single save unit into the ZIP under its ID prefix (`{id}/...`).
///
/// The `id` is `save_unit.id` — a stable, monotonically-assigned identifier
/// that does not change when save units are added or removed. This replaces
/// the former positional index, which could shift and break old archives.
fn append_save_unit<T>(
    writer: &mut ZipWriter<T>,
    save_unit: &SaveUnit,
    preset: CompressionPreset,
    path_ctx: Option<&PathContext>,
) -> Result<(), BackupFileError>
where
    T: Write + Seek,
{
    let id_prefix = PathBuf::from(save_unit.id.to_string());

    if matches!(save_unit.unit_type(), Some(SaveUnitType::WinRegistry)) {
        return append_registry_unit(writer, save_unit, &id_prefix, preset);
    }

    let unit_path = save_unit.resolve_path_for_current_device(path_ctx)?;
    if !unit_path.exists() {
        return Err(BackupFileError::NotExists(unit_path));
    }

    match save_unit
        .unit_type()
        .ok_or(BackupFileError::NonePathError)?
    {
        SaveUnitType::File => {
            let file_name = unit_path
                .file_name()
                .ok_or(BackupFileError::NonePathError)?;
            write_file_entry(writer, &unit_path, &id_prefix.join(file_name), preset)
        }
        SaveUnitType::Folder => {
            let folder_name = unit_path
                .file_name()
                .ok_or(BackupFileError::NonePathError)?;
            add_directory(writer, &unit_path, &id_prefix.join(folder_name), preset)
        }
        SaveUnitType::WinRegistry => unreachable!(),
    }
}

fn ensure_unique_save_unit_ids(save_paths: &[SaveUnit]) -> Result<(), CompressError> {
    let mut seen = HashSet::with_capacity(save_paths.len());
    for save_unit in save_paths {
        if !seen.insert(save_unit.id) {
            return Err(CompressError::Single(BackupFileError::DuplicateSaveUnitId(
                save_unit.id,
            )));
        }
    }
    Ok(())
}

/// Write an origin directory to zip writer.
///
/// `prefix_path` should usually be the origin directory name.
pub(crate) fn add_directory<T>(
    writer: &mut ZipWriter<T>,
    origin: &Path,
    prefix_path: &Path,
    preset: CompressionPreset,
) -> Result<(), BackupFileError>
where
    T: Write + Seek,
{
    write_directory_entry(writer, origin, prefix_path, preset)?;

    for entry in fs::read_dir(origin)? {
        let entry = entry?;
        let entry_path = entry.path();
        let entry_name = prefix_path.join(entry.file_name());

        if entry_path.is_file() {
            write_file_entry(writer, &entry_path, &entry_name, preset)?;
        } else if entry_path.is_dir() {
            add_directory(writer, &entry_path, &entry_name, preset)?;
        }
    }

    Ok(())
}

/// Compress save units to a zip file.
/// Returns the compressed file size in bytes.
pub fn compress_to_file(
    save_paths: &[SaveUnit],
    zip_path: &Path,
    preset: CompressionPreset,
    path_ctx: Option<&PathContext>,
) -> Result<u64, CompressError> {
    ensure_unique_save_unit_ids(save_paths)?;
    let enabled_save_paths: Vec<&SaveUnit> = save_paths
        .iter()
        .filter(|save_unit| save_unit.enabled)
        .collect();

    // Compute source fingerprint for timer dedup (stored in ZIP comment).
    let source_fp =
        crate::backup::state_fingerprint::fingerprint_source_state(save_paths, path_ctx).ok();
    let mut meta = ArchiveMeta::new(preset);
    meta.source_fingerprint = source_fp;

    let file = File::create(zip_path).map_err(|e| CompressError::Single(e.into()))?;
    let mut zip = ZipWriter::new(file);
    zip.set_comment(meta.to_comment());

    let mut compress_errors = Vec::new();
    for save_unit in enabled_save_paths {
        if let Err(err) = append_save_unit(&mut zip, save_unit, preset, path_ctx) {
            compress_errors.push(err);
        }
    }

    zip.finish().map_err(|e| CompressError::Single(e.into()))?;

    if !compress_errors.is_empty() {
        return Err(CompressError::Multiple(compress_errors));
    }

    let file_size = fs::metadata(zip_path)
        .map_err(|e| CompressError::Single(e.into()))?
        .len();
    Ok(file_size)
}
