use std::{
    fs::{self, File},
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
};

use fs_extra::dir::move_dir;
use fs_extra::file::move_file;

use tauri::{AppHandle, Manager};
use zip::{write::FileOptions, ZipWriter};

use crate::{
    config::{SaveUnit, SaveUnitType},
    errors::{BackupFileError, CompressError}, ipc_handler::{IpcNotification, NotificationLevel},
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
        FileOptions::default(),
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
            cur_path = cur_path.join(&entry.file_name());
            if entry_metadata.is_file() {
                let mut f = File::open(&entry_path)?;
                f.read_to_end(&mut buffer)?;
                writer.start_file(
                    cur_path.to_str().ok_or(BackupFileError::NonePathError)?,
                    zip::write::FileOptions::default(),
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
pub fn compress_to_file(save_paths: &[SaveUnit], zip_path: &Path) -> Result<(), CompressError> {
    let file = File::create(zip_path).map_err(|e| CompressError::Single(e.into()))?;
    let mut zip = ZipWriter::new(file);
    let compress_errors: Vec<_> = save_paths
        .iter()
        .map(|x| {
            let unit_path = PathBuf::from(&x.path);
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
                            zip::write::FileOptions::default(),
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
        Result::Ok(())
    }
}

/// Decompress a zip file to their original path
pub fn decompress_from_file(
    save_paths: &[SaveUnit],
    backup_path: &Path,
    date: &str,
    app_handle: &AppHandle,
) -> Result<(), CompressError> {
    let zip_path = backup_path.join([date, ".zip"].concat());
    let file = File::open(zip_path).map_err(|e| CompressError::Single(e.into()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| CompressError::Single(e.into()))?;

    let tmp_folder = PathBuf::from("./tmp"); //TODO: tmp dir
    fs::create_dir_all(&tmp_folder).map_err(|e| CompressError::Single(e.into()))?;
    zip.extract(&tmp_folder)
        .map_err(|e| CompressError::Single(e.into()))?;

    let decompress_errors: Vec<_> = save_paths
        .iter()
        .map(|unit| {
            let unit_path = PathBuf::from(&unit.path); // Target location path
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
                            app_handle // TODO:i18n
                                .emit_all(
                                    "Notification",
                                    IpcNotification {
                                        level: NotificationLevel::warning,
                                        title: "警告".to_string(),
                                        msg: format!("路径 {:?} 不存在，已经自动创建", prefix_root),
                                    },
                                )
                                .map_err(|x| anyhow::Error::from(x))?;
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
                            app_handle// TODO:i18n
                                .emit_all(
                                    "Notification",
                                    IpcNotification {
                                        level: NotificationLevel::warning,
                                        title: "警告".to_string(),
                                        msg: format!("路径 {:?} 不存在，已经自动创建", target_path),
                                    },
                                )
                                .map_err(|x| anyhow::Error::from(x))?;
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
