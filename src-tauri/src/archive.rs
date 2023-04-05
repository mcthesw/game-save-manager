use std::{
    fs::{self, File},
    io::{BufReader, Read, Seek, Write},
    path::PathBuf,
};

use fs_extra::dir::move_dir;
use fs_extra::file::move_file;

use anyhow::{Ok, Result};
use zip::ZipWriter;

use crate::config::{SaveUnit, SaveUnitType};

/// [Code reference](https://github.com/matzefriedrich/zip-extensions-rs/blob/master/src/write.rs#:~:text=%7D-,fn,create_from_directory_with_options,-\()
///
/// Write `origin` folder to zip `writer`, the files will in `prefix_path`
///
/// Normally, `prefix_path` should be the file name of the `origin` folder
fn add_directory<T>(
    writer: &mut ZipWriter<T>,
    origin: &PathBuf,
    prefix_path: &PathBuf,
) -> Result<()>
where
    T: std::io::Write,
    T: Seek,
{
    let mut paths = Vec::new();
    paths.push(origin);

    let mut buffer = Vec::new();

    while let Some(next) = paths.pop() {
        let directory_entry_iter = fs::read_dir(next)?;

        for entry in directory_entry_iter {
            let entry = entry?;
            let entry_path = entry.path();
            let entry_metadata = fs::metadata(&entry_path)?;
            if entry_metadata.is_file() {
                let mut f = File::open(&entry_path)?;
                f.read_to_end(&mut buffer)?;
                let mut cur_path = prefix_path.clone();
                cur_path = cur_path.join(entry.file_name());

                writer.start_file(
                    cur_path.to_str().unwrap(),
                    zip::write::FileOptions::default(),
                )?;
                writer.write_all(&buffer)?;
                buffer.clear();
            } else if entry_metadata.is_dir() {
                let mut cur_path = prefix_path.clone();
                cur_path = cur_path.join(&entry.file_name());
                add_directory(writer, &entry_path, &cur_path)?;
            }
        }
    }

    Ok(())
}

/// Compress a set of save to a zip file in `backup_path` with name 'date.zip'
pub fn compress_to_file(
    save_paths: &Vec<SaveUnit>,
    backup_path: &PathBuf,
    date: &str,
) -> Result<()> {
    let file = File::create(backup_path.join([date, ".zip"].concat()))?;
    let mut zip = ZipWriter::new(file);
    save_paths.into_iter().try_for_each(|x| {
        match x.unit_type {
            SaveUnitType::File => {
                let original_file = File::open(&x.path)?;
                let buf_reader = BufReader::new(original_file);
                zip.start_file(&x.path, zip::write::FileOptions::default())?;
                zip.write_all(buf_reader.buffer())?;
            }
            SaveUnitType::Folder => {
                let path = PathBuf::from(&x.path);
                let root = PathBuf::from(path.file_name().unwrap());
                add_directory(&mut zip, &path, &root)?;
            }
        }
        Ok(())
    })?;
    zip.finish()?;
    Ok(())
}

/// Decompress a zip file to their original path
pub fn decompress_from_file(
    save_paths: &Vec<SaveUnit>,
    backup_path: &PathBuf,
    date: &str,
) -> Result<()> {
    let file_path = backup_path.join([date, ".zip"].concat());
    let tmp_folder = PathBuf::from("./tmp");
    let file = File::open(&file_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    fs::create_dir_all(&tmp_folder)?;
    archive.extract(&tmp_folder)?;
    save_paths
        .iter()
        .try_for_each(|unit| {
            let unit_path = PathBuf::from(&unit.path);
            match unit.unit_type {
                SaveUnitType::File => {
                    let option = fs_extra::file::CopyOptions::new().overwrite(true);
                    let original_path = tmp_folder.join(unit_path.file_name().unwrap());
                    move_file(&original_path, &unit_path, &option)?;
                }
                SaveUnitType::Folder => {
                    let option = fs_extra::dir::CopyOptions::new().overwrite(true);
                    let original_path = tmp_folder.join(unit_path.file_name().unwrap());
                    let target_path = unit_path.parent().unwrap();
                    if !target_path.exists(){
                        fs::create_dir_all(target_path)?;
                    }
                    move_dir(&original_path, &target_path, &option)?;
                }
            }
            Ok(())
        })?;
    fs::remove_dir_all(tmp_folder)?;
    Ok(())
}
