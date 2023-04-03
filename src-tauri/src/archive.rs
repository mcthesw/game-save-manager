use std::{
    fs::{self, File},
    io::{self, BufReader, Write},
    os,
    path::PathBuf,
};

use anyhow::{Error, Ok, Result};
use zip::ZipWriter;
use zip_extensions::write::ZipWriterExtensions;

use crate::config::{SaveUnit, SaveUnitType};

pub fn compress_to_file(
    save_paths: Vec<SaveUnit>,
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
                zip.create_from_directory(&path)?;
            }
        }
        Ok(())
    })?;
    zip.finish()?;
    Ok(())
}

pub fn decompress_from_file(
    save_paths: Vec<SaveUnit>,
    backup_path: &PathBuf,
    date: &str,
) -> Result<()> {
    let file = File::open(backup_path.join([date, ".zip"].concat()))?;
    let mut zip = zip::ZipArchive::new(file)?;
    // TODO: Extract files from zip
    save_paths.iter().try_for_each(|x: &SaveUnit| {
        let target_path = PathBuf::from(&x.path);
        let name = target_path.file_name().unwrap().to_str().unwrap(); //FIXME: how to remove unwrap?
        match x.unit_type {
            SaveUnitType::File => {
                let mut target_file = fs::File::create(&target_path)?;
                let mut backup_file = zip.by_name(&name)?;
                io::copy(&mut backup_file, &mut target_file)?;
            }
            SaveUnitType::Folder => {
                // TODO: handle folder
            }
        }
        Ok(())
    })?;

    Ok(())
}
