use fs_extra::dir::move_dir;
use fs_extra::file::move_file;
use log::warn;
use rust_i18n::t;
use std::{
    fs::{self, File},
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Emitter};
use zip::{ZipWriter, write::SimpleFileOptions};

use crate::{
    backup::{SaveUnit, SaveUnitType},
    device::get_current_device_id,
    ipc_handler::{IpcNotification, NotificationLevel},
    preclude::*,
};

/// [Code reference](https://github.com/matzefriedrich/zip-extensions-rs/blob/master/src/write.rs#:~:text=%7D-,fn,create_from_directory_with_options,-\()
///
/// Write `origin` folder to zip `writer`, the files will in `prefix_path`
///
/// Normally, `prefix_path` should be the file name of the `origin` folder
fn add_directory<T>(
    writer: &mut ZipWriter<T>,
    origin: &PathBuf,
    prefix_path: &Path,
) -> Result<(), BackupFileError>
where
    T: std::io::Write,
    T: Seek,
{
    // Create the folder in zip
    let new_dir_path = prefix_path.to_path_buf();
    writer.add_directory(
        new_dir_path
            .to_str()
            .ok_or(BackupFileError::NonePathError)?
            .to_string(),
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Bzip2),
    )?;
    let mut paths = Vec::new();
    paths.push(origin);

    let mut buffer = Vec::new();

    while let Some(next) = paths.pop() {
        let directory_entry_iter = fs::read_dir(next)?;

        for entry in directory_entry_iter {
            let entry = entry?;
            let entry_path = entry.path();
            let entry_metadata = fs::metadata(&entry_path)?;
            let mut cur_path = prefix_path.to_path_buf();
            cur_path = cur_path.join(entry.file_name());
            if entry_metadata.is_file() {
                let mut f = File::open(&entry_path)?;
                f.read_to_end(&mut buffer)?;
                writer.start_file(
                    cur_path.to_str().ok_or(BackupFileError::NonePathError)?,
                    SimpleFileOptions::default().compression_method(zip::CompressionMethod::Bzip2),
                )?;
                writer.write_all(&buffer)?;
                buffer.clear();
            } else if entry_metadata.is_dir() {
                add_directory(writer, &entry_path, &cur_path)?;
            }
        }
    }

    Ok(())
}

/// Compress a set of save to a zip file in `backup_path` with name 'date.zip'
/// Returns the size of the compressed file in bytes if successful
pub fn compress_to_file(save_paths: &[SaveUnit], zip_path: &Path) -> Result<u64, CompressError> {
    let file = File::create(zip_path).map_err(|e| CompressError::Single(e.into()))?;
    let mut zip = ZipWriter::new(file);
    let compress_errors: Vec<_> = save_paths
        .iter()
        .map(|x| {
            // 获取当前设备 ID，并将 ConfigError 转换为 BackupFileError
            let current_device_id = &get_current_device_id();
            // 获取当前设备的路径，如果不存在则返回 NonePathError
            let unit_path_str = x
                .get_path_for_device(current_device_id)
                .ok_or(BackupFileError::NonePathError)?;

            // 使用 path_resolver 解析路径变量
            let config =
                crate::config::get_config().map_err(|e| BackupFileError::Unexpected(e.into()))?;
            let unit_path = crate::path_resolver::resolve_path(unit_path_str, None, &config)?;
            if unit_path.exists() {
                match x.unit_type {
                    SaveUnitType::File => {
                        let mut original_file = File::open(&unit_path)?;
                        let mut buf = vec![];
                        original_file.read_to_end(&mut buf)?;
                        zip.start_file(
                            unit_path
                                .file_name()
                                .ok_or(BackupFileError::NonePathError)?
                                .to_str()
                                .ok_or(BackupFileError::NonePathError)?,
                            SimpleFileOptions::default()
                                .compression_method(zip::CompressionMethod::Bzip2),
                        )?;
                        zip.write_all(&buf)?;
                    }
                    SaveUnitType::Folder => {
                        let root = PathBuf::from(
                            unit_path
                                .file_name()
                                .ok_or(BackupFileError::NonePathError)?,
                        );
                        add_directory(&mut zip, &unit_path, &root)?;
                    }
                }
            } else {
                Err(BackupFileError::NotExists(unit_path))?;
            }
            Result::<(), BackupFileError>::Ok(())
        })
        .filter_map(|x| x.err())
        .collect();
    zip.finish().map_err(|e| CompressError::Single(e.into()))?;
    if !compress_errors.is_empty() {
        Err(CompressError::Multiple(compress_errors))
    } else {
        // Get the file size after compression
        let file_size = fs::metadata(zip_path)
            .map_err(|e| CompressError::Single(e.into()))?
            .len();
        Result::Ok(file_size)
    }
}

/// Decompress a zip file to their original path
pub fn decompress_from_file(
    save_paths: &[SaveUnit],
    backup_path: &Path,
    date: &str,
    app_handle: Option<&AppHandle>,
) -> Result<(), CompressError> {
    let zip_path = backup_path.join([date, ".zip"].concat());
    let file = File::open(zip_path).map_err(|e| CompressError::Single(e.into()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| CompressError::Single(e.into()))?;

    let tmp_folder = temp_dir::TempDir::new().map_err(|e| CompressError::Single(e.into()))?; // Temporary directory for extraction
    let tmp_folder = tmp_folder.path().to_path_buf(); // Convert to PathBuf for easier manipulation
    fs::create_dir_all(&tmp_folder).map_err(|e| CompressError::Single(e.into()))?;
    zip.extract(&tmp_folder)
        .map_err(|e| CompressError::Single(e.into()))?;

    let decompress_errors: Vec<_> = save_paths
        .iter()
        .map(|unit| {
            // 获取当前设备 ID，并将 ConfigError 转换为 BackupFileError
            let current_device_id = &get_current_device_id();
            // 获取当前设备的路径，如果不存在则返回 NonePathError
            let unit_path_str = unit.get_path_for_device(current_device_id)
                .ok_or(BackupFileError::NonePathError)?;

            // 使用 path_resolver 解析路径变量
            let config = crate::config::get_config().map_err(|e| BackupFileError::Unexpected(e.into()))?;
            let unit_path = crate::path_resolver::resolve_path(unit_path_str, None, &config)?;
            let original_path = tmp_folder.join(
                unit_path
                    .file_name()
                    .ok_or(BackupFileError::NonePathError)?,
            ); // Temp file location path
            if original_path.exists() {
                match unit.unit_type {
                    SaveUnitType::File => {
                        let option = fs_extra::file::CopyOptions::new().overwrite(true);
                        let prefix_root =
                            unit_path.parent().ok_or(BackupFileError::NonePathError)?;
                        if !prefix_root.exists() {
                            // 若文件夹不存在，需要发出警告
                            warn!(target:"rgsm::backup::archive","Path {:#?} not exists, auto created",prefix_root
                                                .to_str()
                                                .unwrap_or("prefix_root.to_str error"));
                            if let Some(app_handle) = app_handle {
                                 app_handle
                                .emit(
                                    "Notification",
                                    IpcNotification {
                                        level: NotificationLevel::warning,
                                        title: "WARNING".to_string(),
                                        msg: t!(
                                            "backend.archive.file_not_exist",
                                            path = prefix_root
                                                .to_str()
                                                .unwrap_or("prefix_root.to_str error")
                                        )
                                        .to_string(),
                                    },
                                )
                                .map_err(anyhow::Error::from)?;
                            }else {
                                // TODO:发出警告?
                            }
                            fs::create_dir_all(prefix_root)?;
                        }
                        if unit.delete_before_apply && unit_path.exists() {
                            fs::remove_file(&unit_path)?;
                        }
                        move_file(original_path, &unit_path, &option)?;
                    }
                    SaveUnitType::Folder => {
                        let option = fs_extra::dir::CopyOptions::new().overwrite(true);
                        let target_path =
                            unit_path.parent().ok_or(BackupFileError::NonePathError)?;
                        if !target_path.exists() {
                            // 若文件夹不存在，需要发出警告
                            warn!(target:"rgsm::backup::archive","Path {:#?} not exists, auto created",target_path
                                                .to_str()
                                                .unwrap_or("prefix_root.to_str error"));
                            if let Some(app_handle) = app_handle {
                            app_handle
                                .emit(
                                    "Notification",
                                    IpcNotification {
                                        level: NotificationLevel::warning,
                                        title: "WARNING".to_string(),
                                        msg: t!(
                                            "backend.archive.file_not_exist",
                                            path = target_path
                                                .to_str()
                                                .unwrap_or("target_path.to_str() error")
                                        )
                                        .to_string(),
                                    },
                                )
                                .map_err(anyhow::Error::from)?;
                            }else{
                                // TODO:发出警告?
                            }
                            fs::create_dir_all(target_path)?;
                        }
                        if unit.delete_before_apply && unit_path.exists() {
                            fs::remove_dir_all(&unit_path)?;
                        }
                        move_dir(original_path, target_path, &option)?;
                    }
                }
            } else {
                Err(BackupFileError::NotExists(original_path))?;
            }
            Result::<(), BackupFileError>::Ok(())
        })
        .filter_map(|x| x.err())
        .collect();
    fs::remove_dir_all(tmp_folder).map_err(|e| CompressError::Single(e.into()))?; //TODO:tmp dir
    if !decompress_errors.is_empty() {
        Err(CompressError::Multiple(decompress_errors))
    } else {
        Result::Ok(())
    }
}
