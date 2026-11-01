use std::{
    fs, io,
    path::Path,
    time::{Duration, SystemTime},
};

use super::restored_directory::remove_restored_directory;

/// A live capture or migration finishes in minutes; anything this old was
/// abandoned by a crash and only wastes disk space.
const STALE_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

pub(super) fn remove_stale_leftovers(directory: &Path) {
    remove_leftovers_older_than(directory, STALE_AFTER);
}

fn remove_leftovers_older_than(directory: &Path, age: Duration) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !(name.ends_with(".capture.tmp") || name.starts_with(".upgrade-")) {
            continue;
        }
        let is_stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|elapsed| elapsed >= age);
        if is_stale {
            let _ = remove_entry(&entry.path());
        }
    }
}

fn remove_entry(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        remove_restored_directory(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_abandoned_capture_and_staging_leftovers_only() {
        let temp = temp_dir::TempDir::new().unwrap();
        let capture = temp.path().join("a.7z.capture.tmp");
        let staging = temp.path().join(".upgrade-1234");
        let archive = temp.path().join("a.7z");
        fs::write(&capture, "partial").unwrap();
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("file"), "x").unwrap();
        fs::write(&archive, "complete").unwrap();

        remove_leftovers_older_than(temp.path(), Duration::from_secs(3600));
        assert!(capture.exists() && staging.exists());

        remove_leftovers_older_than(temp.path(), Duration::ZERO);
        assert!(!capture.exists() && !staging.exists());
        assert!(archive.exists());
    }
}
