use chrono::{Datelike, LocalResult, TimeZone, Timelike, Utc};
use filetime::{FileTime, set_file_mtime};
use fs_extra::dir::move_dir;
use fs_extra::file::move_file;
use log::warn;
use rust_i18n::t;
use std::{
    fs::{self, File},
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};
use tauri::{AppHandle, Emitter};
use zip::{ZipWriter, write::SimpleFileOptions};

use crate::{
    backup::{SaveUnit, SaveUnitType},
    device::get_current_device_id,
    ipc_handler::{IpcNotification, NotificationLevel},
    preclude::*,
};

pub(crate) const ZIP_COMMENT_LOCAL_TIME_MARKER: &str = "RGSM_TS_MODE=LOCAL_V1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ZipTimestampInterpretation {
    LegacyUtc,
    LocalTime,
}

pub(crate) fn zip_timestamp_interpretation_from_comment(
    comment: &[u8],
) -> ZipTimestampInterpretation {
    if comment == ZIP_COMMENT_LOCAL_TIME_MARKER.as_bytes() {
        ZipTimestampInterpretation::LocalTime
    } else {
        ZipTimestampInterpretation::LegacyUtc
    }
}

fn zip_datetime_to_naive_datetime(zip_time: zip::DateTime) -> chrono::NaiveDateTime {
    chrono::NaiveDateTime::new(
        chrono::NaiveDate::from_ymd_opt(
            zip_time.year() as i32,
            zip_time.month() as u32,
            zip_time.day() as u32,
        )
        .unwrap_or_else(|| chrono::NaiveDate::from_ymd_opt(1980, 1, 1).expect("valid date")),
        chrono::NaiveTime::from_hms_opt(
            zip_time.hour() as u32,
            zip_time.minute() as u32,
            zip_time.second() as u32,
        )
        .unwrap_or_default(),
    )
}

pub(crate) fn system_time_to_zip_datetime(system_time: SystemTime) -> zip::DateTime {
    let datetime = chrono::DateTime::<chrono::Local>::from(system_time).naive_local();

    zip::DateTime::from_date_and_time(
        datetime.year() as u16,
        datetime.month() as u8,
        datetime.day() as u8,
        datetime.hour() as u8,
        datetime.minute() as u8,
        datetime.second() as u8,
    )
    .unwrap_or_default()
}

pub(crate) fn zip_datetime_to_system_time(
    zip_time: zip::DateTime,
    interpretation: ZipTimestampInterpretation,
) -> SystemTime {
    let datetime = zip_datetime_to_naive_datetime(zip_time);
    let timestamp = match interpretation {
        ZipTimestampInterpretation::LegacyUtc => datetime.and_utc().timestamp(),
        ZipTimestampInterpretation::LocalTime => {
            local_result_to_timestamp(datetime, chrono::Local.from_local_datetime(&datetime))
        }
    };

    if timestamp < 0 {
        SystemTime::UNIX_EPOCH
    } else {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp as u64)
    }
}

pub(crate) fn local_result_to_timestamp(
    datetime: chrono::NaiveDateTime,
    local_result: LocalResult<chrono::DateTime<chrono::Local>>,
) -> i64 {
    match local_result {
        LocalResult::Single(local_time) => local_time.with_timezone(&Utc).timestamp(),
        LocalResult::Ambiguous(early, _late) => early.with_timezone(&Utc).timestamp(),
        LocalResult::None => datetime.and_utc().timestamp(),
    }
}

/// [Code reference](https://github.com/matzefriedrich/zip-extensions-rs/blob/master/src/write.rs#:~:text=%7D-,fn,create_from_directory_with_options,-\()
///
/// Write `origin` folder to zip `writer`, the files will in `prefix_path`
///
/// Normally, `prefix_path` should be the file name of the `origin` folder
pub(crate) fn add_directory<T>(
    writer: &mut ZipWriter<T>,
    origin: &PathBuf,
    prefix_path: &Path,
) -> Result<(), BackupFileError>
where
    T: std::io::Write,
    T: Seek,
{
    let origin_metadata = fs::metadata(origin)?;
    let dir_mtime = origin_metadata
        .modified()
        .unwrap_or_else(|_| SystemTime::now());
    let dir_datetime = system_time_to_zip_datetime(dir_mtime);

    let new_dir_path = prefix_path.to_path_buf();
    writer.add_directory(
        new_dir_path
            .to_str()
            .ok_or(BackupFileError::NonePathError)?
            .to_string(),
        SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Bzip2)
            .last_modified_time(dir_datetime),
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
                let file_mtime = entry_metadata
                    .modified()
                    .unwrap_or_else(|_| SystemTime::now());
                let file_datetime = system_time_to_zip_datetime(file_mtime);

                let mut f = File::open(&entry_path)?;
                f.read_to_end(&mut buffer)?;
                writer.start_file(
                    cur_path.to_str().ok_or(BackupFileError::NonePathError)?,
                    SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Bzip2)
                        .last_modified_time(file_datetime),
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
    zip.set_comment(ZIP_COMMENT_LOCAL_TIME_MARKER);
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
                        let file_metadata = fs::metadata(&unit_path)?;
                        let file_mtime = file_metadata
                            .modified()
                            .unwrap_or_else(|_| SystemTime::now());
                        let file_datetime = system_time_to_zip_datetime(file_mtime);

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
                                .compression_method(zip::CompressionMethod::Bzip2)
                                .last_modified_time(file_datetime),
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
    let timestamp_interpretation = zip_timestamp_interpretation_from_comment(zip.comment());

    let tmp_folder = temp_dir::TempDir::new().map_err(|e| CompressError::Single(e.into()))?;
    let tmp_folder = tmp_folder.path().to_path_buf();
    fs::create_dir_all(&tmp_folder).map_err(|e| CompressError::Single(e.into()))?;

    let mut dir_timestamps: Vec<(PathBuf, FileTime)> = Vec::new();

    for i in 0..zip.len() {
        let mut file = zip
            .by_index(i)
            .map_err(|e| CompressError::Single(e.into()))?;
        let outpath = tmp_folder.join(file.name());

        if file.is_dir() {
            fs::create_dir_all(&outpath).map_err(|e| CompressError::Single(e.into()))?;
            if let Some(zip_time) = file.last_modified() {
                let system_time = zip_datetime_to_system_time(zip_time, timestamp_interpretation);
                let file_time = FileTime::from_system_time(system_time);
                dir_timestamps.push((outpath.clone(), file_time));
            }
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent).map_err(|e| CompressError::Single(e.into()))?;
            }
            let mut outfile =
                File::create(&outpath).map_err(|e| CompressError::Single(e.into()))?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| CompressError::Single(e.into()))?;
            drop(outfile);

            if let Some(zip_time) = file.last_modified() {
                let system_time = zip_datetime_to_system_time(zip_time, timestamp_interpretation);
                let file_time = FileTime::from_system_time(system_time);
                let _ = set_file_mtime(&outpath, file_time);
            }
        }
    }

    dir_timestamps.sort_by(|a, b| {
        let depth_a = a.0.components().count();
        let depth_b = b.0.components().count();
        depth_b.cmp(&depth_a)
    });
    for (dir_path, file_time) in dir_timestamps {
        let _ = set_file_mtime(&dir_path, file_time);
    }

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
                        let restored_file_mtime = fs::metadata(&original_path)
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .map(FileTime::from_system_time);
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
                        if let Some(file_time) = restored_file_mtime {
                            let _ = set_file_mtime(&unit_path, file_time);
                        }
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
