use std::{
    fs,
    path::{Path, PathBuf},
};
use tokio::sync::MutexGuard;

use crate::config::Config;

pub(crate) fn lock_config_file() -> MutexGuard<'static, ()> {
    crate::config::lock_config_test_file()
}

pub(crate) struct ConfigFileGuard {
    path: PathBuf,
    original_contents: Option<Vec<u8>>,
    owner_backups: Vec<(PathBuf, Option<PathBuf>)>,
    _owner_backup_root: temp_dir::TempDir,
}

impl ConfigFileGuard {
    pub(crate) fn write_default_config() -> Result<Self, Box<dyn std::error::Error>> {
        Self::write_config(&Config::default())
    }

    pub(crate) fn write_config(config: &Config) -> Result<Self, Box<dyn std::error::Error>> {
        let path = crate::app_dirs::resolve_app_path("GameSaveManager.config.json");
        let original_contents = fs::read(&path).ok();
        let owner_paths = [
            crate::config::owner_store::OWNER_DIRECTORY_NAME,
            crate::config::owner_store::OWNER_STAGING_DIRECTORY_NAME,
            crate::config::owner_store::OWNER_ROLLBACK_DIRECTORY_NAME,
        ]
        .into_iter()
        .map(crate::app_dirs::resolve_app_path)
        .collect::<Vec<_>>();
        let owner_backup_root = temp_dir::TempDir::new()?;
        let owner_backups = owner_paths
            .iter()
            .enumerate()
            .map(|(index, owner_path)| {
                if !owner_path.exists() {
                    return Ok((owner_path.clone(), None));
                }
                let backup_path = owner_backup_root.path().join(index.to_string());
                copy_directory(owner_path, &backup_path)?;
                Ok((owner_path.clone(), Some(backup_path)))
            })
            .collect::<Result<Vec<_>, std::io::Error>>()?;
        for owner_path in owner_paths {
            let _ = fs::remove_dir_all(owner_path);
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&path, serde_json::to_vec_pretty(config)?)?;

        Ok(Self {
            path,
            original_contents,
            owner_backups,
            _owner_backup_root: owner_backup_root,
        })
    }
}

impl Drop for ConfigFileGuard {
    fn drop(&mut self) {
        for (owner_path, backup_path) in &self.owner_backups {
            let _ = fs::remove_dir_all(owner_path);
            if let Some(backup_path) = backup_path {
                let _ = copy_directory(backup_path, owner_path);
            }
        }
        if let Some(contents) = &self.original_contents {
            let _ = fs::write(&self.path, contents);
        } else {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn copy_directory(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target_path = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&entry.path(), &target_path)?;
        } else {
            fs::copy(entry.path(), target_path)?;
        }
    }
    Ok(())
}
