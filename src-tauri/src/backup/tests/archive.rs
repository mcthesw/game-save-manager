use super::utils::{ConfigFileGuard, lock_config_file};
use crate::backup::archive::{
    ArchiveVersion, CompressionPreset, V1_COMMENT_MARKER, add_directory, compress_to_file,
    decompress_from_file, local_result_to_timestamp, system_time_to_zip_datetime,
    zip_datetime_to_system_time,
};
use crate::backup::{SaveUnit, SaveUnitType};
use crate::device::get_current_device_id;
use filetime::{FileTime, set_file_mtime};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use zip::{ZipWriter, write::SimpleFileOptions};

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, LocalResult, Timelike};

    fn build_file_save_unit(path: &Path) -> SaveUnit {
        let mut paths = HashMap::new();
        paths.insert(
            get_current_device_id().clone(),
            path.to_string_lossy().to_string(),
        );
        SaveUnit {
            unit_type: SaveUnitType::File,
            paths,
            delete_before_apply: false,
        }
    }

    #[test]
    fn test_timestamp_preservation_file() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = temp_dir::TempDir::new()?;
        let temp_path = temp_dir.path();

        let test_file = temp_path.join("test_file.txt");
        let mut file = File::create(&test_file)?;
        file.write_all(b"test content")?;
        drop(file);

        let past_time = SystemTime::now() - std::time::Duration::from_secs(86400);
        let file_time = FileTime::from_system_time(past_time);
        set_file_mtime(&test_file, file_time)?;

        let original_mtime = fs::metadata(&test_file)?.modified()?;

        let zip_path = temp_path.join("test.zip");
        let zip_file = File::create(&zip_path)?;
        let mut zip_writer = ZipWriter::new(zip_file);

        let file_metadata = fs::metadata(&test_file)?;
        let file_mtime = file_metadata.modified()?;
        let file_datetime = system_time_to_zip_datetime(file_mtime);

        let mut test_file_read = File::open(&test_file)?;
        let mut buf = vec![];
        test_file_read.read_to_end(&mut buf)?;

        zip_writer.start_file(
            "test_file.txt",
            SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Bzip2)
                .last_modified_time(file_datetime),
        )?;
        zip_writer.write_all(&buf)?;
        zip_writer.finish()?;

        let extract_dir = temp_path.join("extract");
        fs::create_dir_all(&extract_dir)?;

        let zip_file = File::open(&zip_path)?;
        let mut zip_archive = zip::ZipArchive::new(zip_file)?;

        for i in 0..zip_archive.len() {
            let mut file = zip_archive.by_index(i)?;
            let outpath = extract_dir.join(file.name());

            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut outfile = File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
            drop(outfile);

            if let Some(zip_time) = file.last_modified() {
                let system_time = zip_datetime_to_system_time(zip_time, ArchiveVersion::V2);
                let file_time = FileTime::from_system_time(system_time);
                set_file_mtime(&outpath, file_time)?;
            }
        }

        let extracted_file = extract_dir.join("test_file.txt");
        let extracted_mtime = fs::metadata(&extracted_file)?.modified()?;

        let original_secs = original_mtime
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();
        let extracted_secs = extracted_mtime
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        assert!(
            (original_secs as i64 - extracted_secs as i64).abs() <= 2,
            "Timestamp should be preserved (within 2 seconds due to MS-DOS timestamp precision)"
        );

        Ok(())
    }

    #[test]
    fn test_timestamp_preservation_directory() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = temp_dir::TempDir::new()?;
        let temp_path = temp_dir.path();

        let test_dir = temp_path.join("test_dir");
        fs::create_dir_all(&test_dir)?;

        let test_file = test_dir.join("nested_file.txt");
        let mut file = File::create(&test_file)?;
        file.write_all(b"nested content")?;
        drop(file);

        let past_time_file = SystemTime::now() - std::time::Duration::from_secs(3600);
        let file_time = FileTime::from_system_time(past_time_file);
        set_file_mtime(&test_file, file_time)?;

        let past_time_dir = SystemTime::now() - std::time::Duration::from_secs(7200);
        let dir_time = FileTime::from_system_time(past_time_dir);
        set_file_mtime(&test_dir, dir_time)?;

        let original_file_mtime = fs::metadata(&test_file)?.modified()?;
        let original_dir_mtime = fs::metadata(&test_dir)?.modified()?;

        let zip_path = temp_path.join("test_dir.zip");
        let zip_file = File::create(&zip_path)?;
        let mut zip_writer = ZipWriter::new(zip_file);

        add_directory(
            &mut zip_writer,
            &test_dir,
            &PathBuf::from("test_dir"),
            CompressionPreset::Standard,
        )?;
        zip_writer.finish()?;

        let extract_dir = temp_path.join("extract");
        fs::create_dir_all(&extract_dir)?;

        let zip_file = File::open(&zip_path)?;
        let mut zip_archive = zip::ZipArchive::new(zip_file)?;

        let mut dir_timestamps: Vec<(PathBuf, FileTime)> = Vec::new();

        for i in 0..zip_archive.len() {
            let mut file = zip_archive.by_index(i)?;
            let outpath = extract_dir.join(file.name());

            if file.is_dir() {
                fs::create_dir_all(&outpath)?;
                if let Some(zip_time) = file.last_modified() {
                    let system_time = zip_datetime_to_system_time(zip_time, ArchiveVersion::V2);
                    let file_time = FileTime::from_system_time(system_time);
                    dir_timestamps.push((outpath.clone(), file_time));
                }
            } else {
                if let Some(parent) = outpath.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut outfile = File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
                drop(outfile);

                if let Some(zip_time) = file.last_modified() {
                    let system_time = zip_datetime_to_system_time(zip_time, ArchiveVersion::V2);
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

        let extracted_file = extract_dir.join("test_dir").join("nested_file.txt");
        let extracted_file_mtime = fs::metadata(&extracted_file)?.modified()?;

        let extracted_dir = extract_dir.join("test_dir");
        let extracted_dir_mtime = fs::metadata(&extracted_dir)?.modified()?;

        let original_file_secs = original_file_mtime
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();
        let extracted_file_secs = extracted_file_mtime
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        let original_dir_secs = original_dir_mtime
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();
        let extracted_dir_secs = extracted_dir_mtime
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        assert!(
            (original_file_secs as i64 - extracted_file_secs as i64).abs() <= 2,
            "File timestamp should be preserved (within 2 seconds)"
        );

        assert!(
            (original_dir_secs as i64 - extracted_dir_secs as i64).abs() <= 2,
            "Directory timestamp should be preserved (within 2 seconds)"
        );

        Ok(())
    }

    #[test]
    fn test_archive_version_from_comment() {
        assert_eq!(
            ArchiveVersion::from_comment(V1_COMMENT_MARKER.as_bytes()),
            ArchiveVersion::V1
        );
        assert_eq!(ArchiveVersion::from_comment(b""), ArchiveVersion::Legacy);
        assert_eq!(
            ArchiveVersion::from_comment(b"something-else"),
            ArchiveVersion::Legacy
        );
    }

    #[test]
    fn test_legacy_utc_timestamp_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let source_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_710_000_001);
        let source_datetime = chrono::DateTime::<chrono::Utc>::from(source_time);
        let zip_time = zip::DateTime::from_date_and_time(
            source_datetime.year() as u16,
            source_datetime.month() as u8,
            source_datetime.day() as u8,
            source_datetime.hour() as u8,
            source_datetime.minute() as u8,
            source_datetime.second() as u8,
        )?;

        let restored_time = zip_datetime_to_system_time(zip_time, ArchiveVersion::Legacy);

        let source_secs = source_time
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();
        let restored_secs = restored_time
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();
        assert!(
            (source_secs as i64 - restored_secs as i64).abs() <= 2,
            "Legacy UTC timestamp should be preserved (within 2 seconds)"
        );
        Ok(())
    }

    #[test]
    fn test_compress_and_decompress_file_end_to_end() -> Result<(), Box<dyn std::error::Error>> {
        let _config_lock = lock_config_file();
        let _config_guard = ConfigFileGuard::write_default_config()?;

        let temp_dir = temp_dir::TempDir::new()?;
        let temp_path = temp_dir.path();
        let backup_dir = temp_path.join("backup");
        fs::create_dir_all(&backup_dir)?;

        let save_file = temp_path.join("profile.sav");
        fs::write(&save_file, b"original-content")?;

        let original_time = SystemTime::now() - Duration::from_secs(12 * 3600 + 45);
        set_file_mtime(&save_file, FileTime::from_system_time(original_time))?;

        let save_unit = build_file_save_unit(&save_file);
        let date = "e2e_snapshot";
        let zip_path = backup_dir.join(format!("{date}.zip"));

        let compressed_size = compress_to_file(
            std::slice::from_ref(&save_unit),
            &zip_path,
            CompressionPreset::Standard,
        )?;
        assert!(compressed_size > 0);

        let zip_file = File::open(&zip_path)?;
        let zip_archive = zip::ZipArchive::new(zip_file)?;
        // V2 comment starts with RGSM_ARCHIVE_V2 header
        let comment = std::str::from_utf8(zip_archive.comment()).unwrap();
        assert!(comment.starts_with("RGSM_ARCHIVE_V2"));
        assert!(comment.contains("\"version\":2"));
        assert!(comment.contains("\"compression\":"));

        fs::write(&save_file, b"mutated-content")?;
        set_file_mtime(&save_file, FileTime::from_system_time(SystemTime::now()))?;

        decompress_from_file(std::slice::from_ref(&save_unit), &backup_dir, date, None)?;

        let restored_content = fs::read(&save_file)?;
        assert_eq!(restored_content, b"original-content");

        let restored_time = fs::metadata(&save_file)?.modified()?;
        let original_secs = original_time
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();
        let restored_secs = restored_time
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();
        assert!(
            (original_secs as i64 - restored_secs as i64).abs() <= 2,
            "End-to-end restore should preserve file timestamp (within 2 seconds), original={original_secs}, restored={restored_secs}"
        );

        Ok(())
    }

    #[test]
    fn test_decompress_legacy_zip_without_comment_uses_utc()
    -> Result<(), Box<dyn std::error::Error>> {
        let _config_lock = lock_config_file();
        let _config_guard = ConfigFileGuard::write_default_config()?;

        let temp_dir = temp_dir::TempDir::new()?;
        let temp_path = temp_dir.path();
        let backup_dir = temp_path.join("backup");
        fs::create_dir_all(&backup_dir)?;

        let save_file = temp_path.join("legacy_profile.sav");
        fs::write(&save_file, b"newer-content")?;

        let date = "legacy_snapshot";
        let zip_path = backup_dir.join(format!("{date}.zip"));
        let expected_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_710_000_001);
        let expected_utc = chrono::DateTime::<chrono::Utc>::from(expected_time);
        let zip_time = zip::DateTime::from_date_and_time(
            expected_utc.year() as u16,
            expected_utc.month() as u8,
            expected_utc.day() as u8,
            expected_utc.hour() as u8,
            expected_utc.minute() as u8,
            expected_utc.second() as u8,
        )?;

        let mut zip_writer = ZipWriter::new(File::create(&zip_path)?);
        zip_writer.start_file(
            save_file
                .file_name()
                .expect("file name must exist")
                .to_str()
                .expect("utf-8 filename required"),
            SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Bzip2)
                .last_modified_time(zip_time),
        )?;
        zip_writer.write_all(b"legacy-content")?;
        zip_writer.finish()?;

        let zip_archive = zip::ZipArchive::new(File::open(&zip_path)?)?;
        assert!(
            zip_archive.comment().is_empty(),
            "Legacy archive should have no timestamp marker"
        );

        let save_unit = build_file_save_unit(&save_file);
        decompress_from_file(std::slice::from_ref(&save_unit), &backup_dir, date, None)?;

        let restored_content = fs::read(&save_file)?;
        assert_eq!(restored_content, b"legacy-content");

        let restored_time = fs::metadata(&save_file)?.modified()?;
        let expected_secs = expected_time
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();
        let restored_secs = restored_time
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();
        assert!(
            (expected_secs as i64 - restored_secs as i64).abs() <= 2,
            "Legacy archive restore should preserve UTC timestamp (within 2 seconds)"
        );

        Ok(())
    }

    #[test]
    fn test_local_result_to_timestamp_prefers_early_ambiguous_value() {
        let naive = chrono::NaiveDate::from_ymd_opt(2024, 11, 3)
            .expect("valid date")
            .and_hms_opt(1, 30, 0)
            .expect("valid time");
        let early = chrono::DateTime::<chrono::Local>::from(
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_730_631_600),
        );
        let late = early + chrono::Duration::hours(1);

        let resolved_ts = local_result_to_timestamp(naive, LocalResult::Ambiguous(early, late));
        assert_eq!(resolved_ts, early.with_timezone(&chrono::Utc).timestamp());
    }

    #[test]
    fn test_local_result_to_timestamp_falls_back_to_utc_when_none() {
        let naive = chrono::NaiveDate::from_ymd_opt(2024, 3, 10)
            .expect("valid date")
            .and_hms_opt(2, 30, 0)
            .expect("valid time");

        let resolved_ts = local_result_to_timestamp(naive, LocalResult::None);
        assert_eq!(resolved_ts, naive.and_utc().timestamp());
    }

    /// Two save units with identically-named files must both survive a
    /// compress → decompress round-trip (V2 index-prefix fix for #144).
    #[test]
    fn test_same_name_files_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let _config_lock = lock_config_file();
        let _config_guard = ConfigFileGuard::write_default_config()?;

        let temp_dir = temp_dir::TempDir::new()?;
        let temp_path = temp_dir.path();

        // Two separate directories each containing a file with the same name
        let dir_a = temp_path.join("unit_a");
        let dir_b = temp_path.join("unit_b");
        fs::create_dir_all(&dir_a)?;
        fs::create_dir_all(&dir_b)?;

        let file_a = dir_a.join("save.dat");
        let file_b = dir_b.join("save.dat");
        fs::write(&file_a, b"content-from-unit-a")?;
        fs::write(&file_b, b"content-from-unit-b")?;

        let unit_a = build_file_save_unit(&file_a);
        let unit_b = build_file_save_unit(&file_b);
        let save_units = [unit_a, unit_b];

        let backup_dir = temp_path.join("backup");
        fs::create_dir_all(&backup_dir)?;
        let date = "collision_test";
        let zip_path = backup_dir.join(format!("{date}.zip"));

        compress_to_file(&save_units, &zip_path, CompressionPreset::Standard)?;

        // Verify the ZIP contains index-prefixed entries
        let zip_file = File::open(&zip_path)?;
        let mut archive = zip::ZipArchive::new(zip_file)?;
        let entry_names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert!(
            entry_names.iter().any(|n| n == "0/save.dat"),
            "Expected entry 0/save.dat, got: {entry_names:?}"
        );
        assert!(
            entry_names.iter().any(|n| n == "1/save.dat"),
            "Expected entry 1/save.dat, got: {entry_names:?}"
        );

        // Mutate files to confirm restore works
        fs::write(&file_a, b"mutated")?;
        fs::write(&file_b, b"mutated")?;

        decompress_from_file(&save_units, &backup_dir, date, None)?;

        assert_eq!(fs::read(&file_a)?, b"content-from-unit-a");
        assert_eq!(fs::read(&file_b)?, b"content-from-unit-b");

        Ok(())
    }

    /// All compression presets must produce valid archives that decompress correctly.
    #[test]
    fn test_all_compression_presets() -> Result<(), Box<dyn std::error::Error>> {
        let _config_lock = lock_config_file();
        let _config_guard = ConfigFileGuard::write_default_config()?;

        let presets = [
            CompressionPreset::Store,
            CompressionPreset::Fast,
            CompressionPreset::Standard,
            CompressionPreset::Best,
        ];

        for preset in presets {
            let temp_dir = temp_dir::TempDir::new()?;
            let temp_path = temp_dir.path();

            let save_file = temp_path.join("data.bin");
            fs::write(&save_file, b"hello world from preset test")?;

            let save_unit = build_file_save_unit(&save_file);
            let backup_dir = temp_path.join("backup");
            fs::create_dir_all(&backup_dir)?;
            let date = "preset_test";
            let zip_path = backup_dir.join(format!("{date}.zip"));

            let size = compress_to_file(std::slice::from_ref(&save_unit), &zip_path, preset)?;
            assert!(size > 0, "Preset {preset:?} produced empty archive");

            // Mutate and restore
            fs::write(&save_file, b"overwritten")?;
            decompress_from_file(std::slice::from_ref(&save_unit), &backup_dir, date, None)?;

            let restored = fs::read(&save_file)?;
            assert_eq!(
                restored, b"hello world from preset test",
                "Preset {preset:?} failed to roundtrip"
            );
        }

        Ok(())
    }
}
