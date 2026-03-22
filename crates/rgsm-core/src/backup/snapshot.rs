use serde::{Deserialize, Serialize};
use specta::Type;

use crate::default_value;
use crate::device::DeviceId;

/// Tracks how a snapshot was created.
///
/// Forward-compatible: unknown variants from future versions deserialize
/// as `Unknown` — these are never auto-deleted by cleanup logic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
pub enum CreatedBy {
    /// User-created snapshot (manual backup, IPC call, etc.)
    #[default]
    Manual,
    /// Created by the auto-backup timer, subject to retention policy cleanup.
    Timer,
    /// Created via the system tray quick action.
    Tray,
    /// Created via a global hotkey quick action.
    Hotkey,
    /// Forward-compat catch-all for variants added in future versions.
    #[serde(other)]
    Unknown,
}

/// A backup is a zip file that contains
/// all the file that the save unit has declared.
/// The date is the unique indicator for a backup
#[derive(Debug, Serialize, Deserialize, Type, Clone)]
pub struct Snapshot {
    pub date: String,
    pub describe: String,
    pub path: String, // like "D:\\SaveManager\save_data\Game1\date.zip"
    #[serde(default = "default_value::default_zero")]
    pub size: u64, // in bytes
    /// Parent snapshot's date (None means this is a root node)
    #[serde(default = "default_value::default_none")]
    pub parent: Option<String>,
    /// XXH3 hash of the archive file for integrity verification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_hash: Option<String>,
    /// The device that created this snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<DeviceId>,
    /// How this snapshot was created.
    #[serde(default)]
    pub created_by: CreatedBy,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_by_serde_roundtrip() {
        for (variant, expected) in [
            (CreatedBy::Manual, r#""Manual""#),
            (CreatedBy::Timer, r#""Timer""#),
            (CreatedBy::Tray, r#""Tray""#),
            (CreatedBy::Hotkey, r#""Hotkey""#),
        ] {
            assert_eq!(serde_json::to_string(&variant).unwrap(), expected);
            assert_eq!(
                serde_json::from_str::<CreatedBy>(expected).unwrap(),
                variant
            );
        }
    }

    #[test]
    fn created_by_unknown_variant_forward_compat() {
        // Future variants deserialize as Unknown (not an error)
        let result: CreatedBy = serde_json::from_str(r#""ProcessDetected""#).unwrap();
        assert_eq!(result, CreatedBy::Unknown);
    }

    #[test]
    fn created_by_default_is_manual() {
        assert_eq!(CreatedBy::default(), CreatedBy::Manual);
    }

    #[test]
    fn snapshot_missing_created_by_defaults_to_manual() {
        let json = r#"{
            "date": "2025-01-01T00:00:00",
            "describe": "test",
            "path": "/tmp/test.zip",
            "size": 100
        }"#;
        let snap: Snapshot = serde_json::from_str(json).unwrap();
        assert_eq!(snap.created_by, CreatedBy::Manual);
    }
}
