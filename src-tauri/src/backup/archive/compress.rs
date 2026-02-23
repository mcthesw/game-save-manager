use std::{
    fs::{self, File},
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use zip::{ZipWriter, write::SimpleFileOptions};

use crate::backup::path_format::path_to_zip_style;
use crate::{
    backup::{SaveUnit, SaveUnitType},
    preclude::*,
};

use super::timestamp::{ZIP_COMMENT_LOCAL_TIME_MARKER, system_time_to_zip_datetime};

fn file_modified_to_zip_datetime(path: &Path) -> Result<zip::DateTime, BackupFileError> {
    let modified = fs::metadata(path)?
        .modified()
        .unwrap_or_else(|_| SystemTime::now());
    Ok(system_time_to_zip_datetime(modified))
}

fn zip_options_with_mtime(path: &Path) -> Result<SimpleFileOptions, BackupFileError> {
    let mtime = file_modified_to_zip_datetime(path)?;
    Ok(SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Bzip2)
        .last_modified_time(mtime))
}

fn write_file_entry<T>(
    writer: &mut ZipWriter<T>,
    source_path: &Path,
    entry_name: &Path,
) -> Result<(), BackupFileError>
where
    T: Write + Seek,
{
    let mut file = File::open(source_path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    writer.start_file(
        path_to_zip_style(entry_name)?,
        zip_options_with_mtime(source_path)?,
    )?;
    writer.write_all(&buffer)?;
    Ok(())
}

fn write_directory_entry<T>(
    writer: &mut ZipWriter<T>,
    source_path: &Path,
    entry_name: &Path,
) -> Result<(), BackupFileError>
where
    T: Write + Seek,
{
    writer.add_directory(
        path_to_zip_style(entry_name)?,
        zip_options_with_mtime(source_path)?,
    )?;
    Ok(())
}

fn append_save_unit<T>(
    writer: &mut ZipWriter<T>,
    save_unit: &SaveUnit,
) -> Result<(), BackupFileError>
where
    T: Write + Seek,
{
    let unit_path = save_unit.resolve_path_for_current_device()?;
    if !unit_path.exists() {
        return Err(BackupFileError::NotExists(unit_path));
    }

    match save_unit.unit_type {
        SaveUnitType::File => {
            let file_name = unit_path
                .file_name()
                .ok_or(BackupFileError::NonePathError)?;
            write_file_entry(writer, &unit_path, &PathBuf::from(file_name))
        }
        SaveUnitType::Folder => {
            let folder_name = unit_path
                .file_name()
                .ok_or(BackupFileError::NonePathError)?;
            add_directory(writer, &unit_path, &PathBuf::from(folder_name))
        }
    }
}

/// Write an origin directory to zip writer.
///
/// `prefix_path` should usually be the origin directory name.
pub(crate) fn add_directory<T>(
    writer: &mut ZipWriter<T>,
    origin: &Path,
    prefix_path: &Path,
) -> Result<(), BackupFileError>
where
    T: Write + Seek,
{
    write_directory_entry(writer, origin, prefix_path)?;

    for entry in fs::read_dir(origin)? {
        let entry = entry?;
        let entry_path = entry.path();
        let entry_name = prefix_path.join(entry.file_name());

        if entry_path.is_file() {
            write_file_entry(writer, &entry_path, &entry_name)?;
        } else if entry_path.is_dir() {
            add_directory(writer, &entry_path, &entry_name)?;
        }
    }

    Ok(())
}

/// Compress save units to a zip file.
/// Returns the compressed file size in bytes.
pub fn compress_to_file(save_paths: &[SaveUnit], zip_path: &Path) -> Result<u64, CompressError> {
    let file = File::create(zip_path).map_err(|e| CompressError::Single(e.into()))?;
    let mut zip = ZipWriter::new(file);
    zip.set_comment(ZIP_COMMENT_LOCAL_TIME_MARKER);

    let mut compress_errors = Vec::new();
    for save_unit in save_paths {
        if let Err(err) = append_save_unit(&mut zip, save_unit) {
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
