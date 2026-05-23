use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

const SETTINGS_FILE: &str = "rgsm-tui.settings.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TuiSettings {
    #[serde(default)]
    pub auto_enqueue_cloud_on_change: bool,
    #[serde(default = "default_true")]
    pub ludusavi_local_only: bool,
}

impl Default for TuiSettings {
    fn default() -> Self {
        Self {
            auto_enqueue_cloud_on_change: false,
            ludusavi_local_only: true,
        }
    }
}

impl TuiSettings {
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join(SETTINGS_FILE)
    }

    pub fn load(data_dir: &Path) -> Result<Self> {
        let path = Self::path(data_dir);
        if !path.exists() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }

    pub fn save(&self, data_dir: &Path) -> Result<()> {
        fs::create_dir_all(data_dir)?;
        fs::write(Self::path(data_dir), serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_disables_auto_cloud_enqueue() {
        assert!(!TuiSettings::default().auto_enqueue_cloud_on_change);
    }

    #[test]
    fn default_limits_ludusavi_to_local_games() {
        assert!(TuiSettings::default().ludusavi_local_only);
    }

    #[test]
    fn missing_ludusavi_scope_uses_local_only_default() {
        let loaded: TuiSettings =
            serde_json::from_str(r#"{"auto_enqueue_cloud_on_change":true}"#).unwrap();

        assert!(loaded.auto_enqueue_cloud_on_change);
        assert!(loaded.ludusavi_local_only);
    }

    #[test]
    fn settings_round_trip_without_touching_domain_config() {
        let temp = temp_dir::TempDir::new().unwrap();
        let settings = TuiSettings {
            auto_enqueue_cloud_on_change: true,
            ludusavi_local_only: false,
        };

        settings.save(temp.path()).unwrap();
        let loaded = TuiSettings::load(temp.path()).unwrap();

        assert_eq!(loaded, settings);
        assert!(!temp.path().join("GameSaveManager.config.json").exists());
    }
}
