use std::path::{Path, PathBuf};

use crate::backup::{ArchiveFormat, Snapshot};

pub fn archive_file_name(date: &str, format: ArchiveFormat) -> String {
    format!("{date}.{}", format.extension())
}

pub fn archive_path(game_dir: &Path, date: &str, format: ArchiveFormat) -> PathBuf {
    game_dir.join(archive_file_name(date, format))
}

pub fn snapshot_archive_path(game_dir: &Path, snapshot: &Snapshot) -> PathBuf {
    let persisted = PathBuf::from(&snapshot.path);
    let persisted_matches_format = persisted
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case(snapshot.archive_format.extension())
        });
    if !snapshot.path.is_empty() && persisted_matches_format && persisted.exists() {
        return persisted;
    }
    archive_path(game_dir, &snapshot.date, snapshot.archive_format)
}

pub fn remote_archive_path(storage_key: &str, date: &str, format: ArchiveFormat) -> PathBuf {
    PathBuf::from("save_data")
        .join(storage_key)
        .join(archive_file_name(date, format))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{CreatedBy, Snapshot};

    fn snapshot(format: ArchiveFormat) -> Snapshot {
        Snapshot {
            date: "2026-07-13T00-00-00".into(),
            describe: String::new(),
            path: String::new(),
            archive_format: format,
            size: 0,
            parent: None,
            archive_hash: None,
            device_id: None,
            created_by: CreatedBy::Manual,
        }
    }

    #[test]
    fn builds_format_specific_names_from_one_owner() {
        assert_eq!(
            archive_file_name("snapshot", ArchiveFormat::Zip),
            "snapshot.zip"
        );
        assert_eq!(
            archive_file_name("snapshot", ArchiveFormat::SevenZ),
            "snapshot.7z"
        );
    }

    #[test]
    fn empty_persisted_path_uses_declared_format() {
        let path = snapshot_archive_path(Path::new("game"), &snapshot(ArchiveFormat::SevenZ));
        assert_eq!(path, Path::new("game").join("2026-07-13T00-00-00.7z"));
    }

    #[test]
    fn mismatched_persisted_extension_does_not_override_declared_format() {
        let mut snapshot = snapshot(ArchiveFormat::SevenZ);
        snapshot.path = "legacy.zip".into();
        assert_eq!(
            snapshot_archive_path(Path::new("game"), &snapshot),
            Path::new("game").join("2026-07-13T00-00-00.7z")
        );
    }

    #[test]
    fn remote_path_uses_declared_format() {
        assert_eq!(
            remote_archive_path("game", "snapshot", ArchiveFormat::SevenZ),
            Path::new("save_data").join("game").join("snapshot.7z")
        );
    }
}
