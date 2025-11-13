use crate::backup::archive::{add_directory, system_time_to_zip_datetime, zip_datetime_to_system_time};
use filetime::{set_file_mtime, FileTime};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
    time::SystemTime,
};
use zip::{ZipWriter, write::SimpleFileOptions};

#[cfg(test)]
mod tests {
    use super::*;

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
                let system_time = zip_datetime_to_system_time(zip_time);
                let file_time = FileTime::from_system_time(system_time);
                set_file_mtime(&outpath, file_time)?;
            }
        }
        
        let extracted_file = extract_dir.join("test_file.txt");
        let extracted_mtime = fs::metadata(&extracted_file)?.modified()?;
        
        let original_secs = original_mtime.duration_since(SystemTime::UNIX_EPOCH)?.as_secs();
        let extracted_secs = extracted_mtime.duration_since(SystemTime::UNIX_EPOCH)?.as_secs();
        
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
        
        add_directory(&mut zip_writer, &test_dir, &PathBuf::from("test_dir"))?;
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
                    let system_time = zip_datetime_to_system_time(zip_time);
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
                    let system_time = zip_datetime_to_system_time(zip_time);
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
        
        let original_file_secs = original_file_mtime.duration_since(SystemTime::UNIX_EPOCH)?.as_secs();
        let extracted_file_secs = extracted_file_mtime.duration_since(SystemTime::UNIX_EPOCH)?.as_secs();
        
        let original_dir_secs = original_dir_mtime.duration_since(SystemTime::UNIX_EPOCH)?.as_secs();
        let extracted_dir_secs = extracted_dir_mtime.duration_since(SystemTime::UNIX_EPOCH)?.as_secs();
        
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
}
