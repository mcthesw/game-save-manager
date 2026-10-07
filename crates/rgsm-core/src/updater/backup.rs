use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

/// Retain each distinct pre-migration document without overwriting an earlier
/// backup. This is local storage only; callers must preserve before replacing.
pub(crate) fn preserve_original(path: &Path, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let mut destination = path.with_extension("json.bak");
    if destination.exists() {
        if fs::read(&destination)? == bytes {
            return Ok(destination);
        }
        destination = path.with_extension(format!(
            "json.{:016x}.bak",
            xxhash_rust::xxh3::xxh3_64(bytes)
        ));
    }
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
    {
        Ok(mut file) => {
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if fs::read(&destination)? != bytes {
                return Err(error);
            }
        }
        Err(error) => return Err(error),
    }
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retries_and_later_upgrades_preserve_each_original() {
        let directory = temp_dir::TempDir::new().unwrap();
        let path = directory.path().join("Backups.json");
        let first = preserve_original(&path, b"before-first-upgrade").unwrap();
        let second = preserve_original(&path, b"before-second-upgrade").unwrap();
        assert_ne!(first, second);
        assert_eq!(
            preserve_original(&path, b"before-second-upgrade").unwrap(),
            second
        );
        assert_eq!(fs::read(first).unwrap(), b"before-first-upgrade");
        assert_eq!(fs::read(second).unwrap(), b"before-second-upgrade");
    }
}
