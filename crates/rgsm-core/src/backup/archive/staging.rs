use std::{
    collections::BTreeSet,
    fs::{self, File},
    path::{Path, PathBuf},
};

use crate::backup::ArchiveFormat;

use super::{
    ArchiveVersion, migration::ArchiveMigrationError, portable::validate_relative_name,
    timestamp::zip_datetime_to_system_time,
};

pub(super) struct StagingDirectory(pub PathBuf);

impl StagingDirectory {
    pub fn new(parent: &Path) -> std::io::Result<Self> {
        let path = parent.join(format!(".upgrade-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        #[cfg(windows)]
        #[expect(
            clippy::permissions_set_readonly_false,
            reason = "Windows read-only attribute; Unix cleanup uses owner-only directory permissions"
        )]
        for entry in walkdir::WalkDir::new(&self.0)
            .into_iter()
            .filter_map(Result::ok)
        {
            if let Ok(metadata) = entry.metadata() {
                let mut permissions = metadata.permissions();
                if permissions.readonly() {
                    permissions.set_readonly(false);
                    let _ = fs::set_permissions(entry.path(), permissions);
                }
            }
        }
        let _ = super::restored_directory::remove_restored_directory(&self.0);
    }
}

pub(super) fn extract(
    path: &Path,
    format: ArchiveFormat,
    destination: &Path,
) -> Result<(), ArchiveMigrationError> {
    match format {
        ArchiveFormat::Zip => extract_zip(path, destination),
        ArchiveFormat::SevenZ => extract_seven_z(path, destination),
    }
}

fn target(
    root: &Path,
    name: &str,
    names: &mut BTreeSet<String>,
) -> Result<PathBuf, ArchiveMigrationError> {
    let name = name.trim_end_matches(['/', '\\']).replace('\\', "/");
    validate_relative_name(&name)?;
    if !names.insert(name.to_lowercase()) {
        return Err(ArchiveMigrationError::Invalid(
            "duplicate archive entry".into(),
        ));
    }
    Ok(root.join(name))
}

fn extract_zip(path: &Path, destination: &Path) -> Result<(), ArchiveMigrationError> {
    let mut archive = zip::ZipArchive::new(File::open(path)?).map_err(invalid)?;
    let version = ArchiveVersion::from_comment(archive.comment());
    let mut names = BTreeSet::new();
    let mut directories = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(invalid)?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(ArchiveMigrationError::Invalid(
                "symbolic link in archive".into(),
            ));
        }
        let path = target(destination, entry.name(), &mut names)?;
        let time = entry.last_modified().map(|time| {
            filetime::FileTime::from_system_time(zip_datetime_to_system_time(time, version))
        });
        if entry.is_dir() {
            fs::create_dir_all(&path)?;
            directories.push((path, time));
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = File::create(&path)?;
            std::io::copy(&mut entry, &mut file)?;
            drop(file);
            if let Some(time) = time {
                filetime::set_file_mtime(&path, time)?;
            }
            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(mode & 0o7777))?;
            }
        }
    }
    directories.sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
    for (path, time) in directories {
        if let Some(time) = time {
            filetime::set_file_mtime(path, time)?;
        }
    }
    Ok(())
}

fn extract_seven_z(path: &Path, destination: &Path) -> Result<(), ArchiveMigrationError> {
    let mut reader = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())
        .map_err(invalid)?;
    let mut names = BTreeSet::new();
    let mut directories = Vec::new();
    reader
        .for_each_entries(|entry, input| {
            let path = target(destination, entry.name(), &mut names)
                .map_err(|error| sevenz_error(error.to_string()))?;
            if entry.is_directory {
                fs::create_dir_all(&path)?;
                directories.push((path, entry.clone()));
            } else {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut file = File::create(&path)?;
                std::io::copy(input, &mut file)?;
                drop(file);
                super::seven_z::apply_entry_metadata(&path, entry)?;
            }
            Ok(true)
        })
        .map_err(invalid)?;
    directories.sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
    for (path, entry) in directories {
        super::seven_z::apply_entry_metadata(&path, &entry).map_err(invalid)?;
    }
    Ok(())
}

fn invalid(error: impl std::fmt::Display) -> ArchiveMigrationError {
    ArchiveMigrationError::Invalid(error.to_string())
}

fn sevenz_error(message: impl Into<String>) -> sevenz_rust2::Error {
    std::io::Error::other(message.into()).into()
}
