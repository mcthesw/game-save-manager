use serde::{Deserialize, Serialize};
use specta::Type;

use crate::default_value;
use crate::device::DeviceId;

/// Container format used by a Snapshot archive.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFormat {
    /// Historical ZIP Archive Legacy/V1/V2/V3.
    #[default]
    Zip,
    /// Metadata-faithful 7z Archive V4.
    SevenZ,
}

impl ArchiveFormat {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::SevenZ => "7z",
        }
    }
}

/// Tracks how a snapshot was created.
///
/// Forward-compatible: unknown variants from future versions deserialize
/// as `Unknown` — these are never auto-deleted by cleanup logic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema, Default)]
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
    /// Created when a monitored game process starts.
    ProcessStart,
    /// Created when a monitored game process exits.
    ProcessExit,
    /// Created by a timer while a monitored game process is running.
    ProcessInterval,
    /// Forward-compat catch-all for variants added in future versions.
    #[serde(other)]
    Unknown,
}

impl CreatedBy {
    pub fn is_automatic_backup(&self) -> bool {
        matches!(
            self,
            CreatedBy::Timer
                | CreatedBy::ProcessStart
                | CreatedBy::ProcessExit
                | CreatedBy::ProcessInterval
        )
    }
}

/// A backup archive containing all data declared by its Save Units.
/// The date is the unique indicator for a backup
#[derive(Debug, Serialize, Deserialize, Type, utoipa::ToSchema, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub date: String,
    pub describe: String,
    pub path: String,
    /// Archive container. Missing values in historical Backups.json default to ZIP.
    #[serde(default)]
    pub archive_format: ArchiveFormat,
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
    fn legacy_snapshot_without_archive_format_defaults_to_zip() {
        let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
            "date": "2026-07-13T00-00-00",
            "describe": "legacy",
            "path": "save_data/game/2026-07-13T00-00-00.zip",
            "size": 1
        }))
        .unwrap();

        assert_eq!(snapshot.archive_format, ArchiveFormat::Zip);
    }

    #[test]
    fn archive_format_extensions_are_stable() {
        assert_eq!(ArchiveFormat::Zip.extension(), "zip");
        assert_eq!(ArchiveFormat::SevenZ.extension(), "7z");
    }

    #[test]
    fn created_by_serde_roundtrip() {
        for (variant, expected) in [
            (CreatedBy::Manual, r#""Manual""#),
            (CreatedBy::Timer, r#""Timer""#),
            (CreatedBy::Tray, r#""Tray""#),
            (CreatedBy::Hotkey, r#""Hotkey""#),
            (CreatedBy::ProcessStart, r#""ProcessStart""#),
            (CreatedBy::ProcessExit, r#""ProcessExit""#),
            (CreatedBy::ProcessInterval, r#""ProcessInterval""#),
        ] {
            assert_eq!(serde_json::to_string(&variant).unwrap(), expected);
            assert_eq!(
                serde_json::from_str::<CreatedBy>(expected).unwrap(),
                variant
            );
        }
    }

    #[test]
    fn automatic_backup_sources_are_marked() {
        assert!(CreatedBy::Timer.is_automatic_backup());
        assert!(CreatedBy::ProcessStart.is_automatic_backup());
        assert!(CreatedBy::ProcessExit.is_automatic_backup());
        assert!(CreatedBy::ProcessInterval.is_automatic_backup());
        assert!(!CreatedBy::Manual.is_automatic_backup());
        assert!(!CreatedBy::Tray.is_automatic_backup());
        assert!(!CreatedBy::Hotkey.is_automatic_backup());
        assert!(!CreatedBy::Unknown.is_automatic_backup());
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
