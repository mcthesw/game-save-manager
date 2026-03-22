use std::{fs, path::PathBuf};
use tokio::sync::MutexGuard;

use crate::config::Config;

pub(crate) fn lock_config_file() -> MutexGuard<'static, ()> {
    crate::config::lock_config_test_file()
}

pub(crate) struct ConfigFileGuard {
    path: PathBuf,
    original_contents: Option<Vec<u8>>,
}

impl ConfigFileGuard {
    pub(crate) fn write_default_config() -> Result<Self, Box<dyn std::error::Error>> {
        Self::write_config(&Config::default())
    }

    pub(crate) fn write_config(config: &Config) -> Result<Self, Box<dyn std::error::Error>> {
        let path = crate::app_dirs::resolve_app_path("GameSaveManager.config.json");
        let original_contents = fs::read(&path).ok();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&path, serde_json::to_vec_pretty(config)?)?;

        Ok(Self {
            path,
            original_contents,
        })
    }
}

impl Drop for ConfigFileGuard {
    fn drop(&mut self) {
        if let Some(contents) = &self.original_contents {
            let _ = fs::write(&self.path, contents);
        } else {
            let _ = fs::remove_file(&self.path);
        }
    }
}
