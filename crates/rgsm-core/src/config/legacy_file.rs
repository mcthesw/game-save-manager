use std::fs;
use std::path::Path;

use log::warn;

/// Retire the pre-owner-store `GameSaveManager.config.json` once the owner store
/// holds the data, so that no stale copy is left next to the authoritative state.
///
/// The pre-upgrade original lives in `GameSaveManager.config.json.bak`: it is the
/// only file an older release can be restored from by renaming it back. An
/// existing `.bak` is never overwritten, because it is older than the `.json`
/// (which may already have been migrated). Without one, the `.json` becomes the
/// `.bak`, unless it was just created for a fresh install and has no original
/// worth keeping.
///
/// Failures are logged only; a leftover file must not block startup.
pub(super) fn retire_legacy_config(path: &Path, is_original: bool) {
    if let Err(error) = retire(path, is_original) {
        warn!(
            "Failed to retire legacy config file {}: {error}",
            path.display()
        );
    }
}

fn retire(path: &Path, is_original: bool) -> std::io::Result<()> {
    if !path.is_file() {
        return Ok(());
    }
    let backup = path.with_extension("json.bak");
    if is_original && !backup.exists() {
        fs::rename(path, backup)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAME: &str = "GameSaveManager.config.json";

    #[test]
    fn keeps_existing_backup_and_removes_stale_config() {
        let root = temp_dir::TempDir::new().unwrap();
        let config = root.path().join(NAME);
        let backup = root.path().join(format!("{NAME}.bak"));
        fs::write(&config, "migrated").unwrap();
        fs::write(&backup, "original").unwrap();

        retire_legacy_config(&config, true);

        assert!(!config.exists());
        assert_eq!(fs::read_to_string(backup).unwrap(), "original");
    }

    #[test]
    fn turns_config_into_backup_when_none_exists() {
        let root = temp_dir::TempDir::new().unwrap();
        let config = root.path().join(NAME);
        fs::write(&config, "original").unwrap();

        retire_legacy_config(&config, true);

        assert!(!config.exists());
        assert_eq!(
            fs::read_to_string(root.path().join(format!("{NAME}.bak"))).unwrap(),
            "original"
        );
    }

    #[test]
    fn leaves_no_backup_for_a_fresh_install() {
        let root = temp_dir::TempDir::new().unwrap();
        let config = root.path().join(NAME);
        fs::write(&config, "default").unwrap();

        retire_legacy_config(&config, false);

        assert!(!config.exists());
        assert!(!root.path().join(format!("{NAME}.bak")).exists());
    }

    #[test]
    fn missing_config_is_ignored() {
        let root = temp_dir::TempDir::new().unwrap();

        retire_legacy_config(&root.path().join(NAME), true);

        assert!(!root.path().join(format!("{NAME}.bak")).exists());
    }
}
