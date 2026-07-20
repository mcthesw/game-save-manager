use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use specta::Type;

use super::Snapshot;
use crate::default_value;
use crate::device::{DeviceId, get_current_device_id};

/// A backup list info is a json file in a backup folder for a game.
/// It contains the name of the game,
/// and all backups' path
#[derive(Debug, Serialize, Deserialize, Type, Clone, PartialEq, Eq)]
pub struct GameSnapshots {
    pub name: String,
    pub backups: Vec<Snapshot>,
    /// Device-specific HEAD pointers. Each device tracks which snapshot new
    /// snapshots should branch from locally.
    #[serde(default = "default_value::empty_map")]
    pub device_heads: HashMap<DeviceId, String>,
    /// Legacy single-head field kept only for backward-compatible deserialization.
    #[serde(
        default = "default_value::default_none",
        rename = "head",
        skip_serializing
    )]
    #[specta(skip)]
    legacy_head: Option<String>,
    /// Monotonically increasing version for sync conflict detection.
    #[serde(default)]
    pub sync_version: u64,
    /// The device that last modified this metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_device: Option<DeviceId>,
    /// ISO 8601 timestamp of the last sync operation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_timestamp: Option<String>,
}

impl GameSnapshots {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            backups: Vec::new(),
            device_heads: HashMap::new(),
            legacy_head: None,
            sync_version: 0,
            last_sync_device: None,
            last_sync_timestamp: None,
        }
    }

    pub fn forget_v2_tombstones(&mut self, snapshot_ids: &BTreeSet<String>) -> usize {
        let previous = self.backups.len();
        self.backups
            .retain(|snapshot| !snapshot_ids.contains(&snapshot.date));
        self.device_heads
            .retain(|_, head| !snapshot_ids.contains(head));
        previous - self.backups.len()
    }

    pub fn normalize_heads(&mut self) {
        self.normalize_heads_for_device(get_current_device_id());
    }

    /// Normalize legacy single-head metadata with an explicit fallback Device.
    ///
    /// Cutover uses this form so graph migration never consults process-global
    /// Device state. The legacy Snapshot `device_id` is consumed only while
    /// deriving a Head and is not part of the V2 Snapshot identity.
    pub fn normalize_heads_for_device(&mut self, fallback_device_id: &DeviceId) {
        if self.device_heads.is_empty()
            && let Some(legacy_head) = self.legacy_head.take()
        {
            let owner = self
                .last_sync_device
                .clone()
                .or_else(|| {
                    self.backups
                        .iter()
                        .find(|snapshot| snapshot.date == legacy_head)
                        .and_then(|snapshot| snapshot.device_id.clone())
                })
                .unwrap_or_else(|| fallback_device_id.clone());
            self.device_heads.insert(owner, legacy_head);
        }

        self.device_heads
            .retain(|_, head| self.backups.iter().any(|snapshot| snapshot.date == *head));
        self.legacy_head = None;
    }

    pub fn head_for_device(&self, device_id: &DeviceId) -> Option<&String> {
        self.device_heads.get(device_id)
    }

    pub fn current_device_head(&self) -> Option<&String> {
        self.head_for_device(get_current_device_id())
    }

    pub fn current_device_head_cloned(&self) -> Option<String> {
        self.current_device_head().cloned()
    }

    pub fn set_head_for_device(&mut self, device_id: DeviceId, head: Option<String>) {
        match head {
            Some(head) => {
                self.device_heads.insert(device_id, head);
            }
            None => {
                self.device_heads.remove(&device_id);
            }
        }
        self.legacy_head = None;
    }

    pub fn set_current_device_head(&mut self, head: Option<String>) {
        self.set_head_for_device(get_current_device_id().clone(), head);
    }

    pub fn head_entries(&self) -> impl Iterator<Item = (&DeviceId, &String)> {
        self.device_heads.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(date: &str, device_id: Option<&str>) -> Snapshot {
        Snapshot {
            date: date.to_string(),
            describe: String::new(),
            path: String::new(),
            archive_format: crate::backup::ArchiveFormat::Zip,
            size: 0,
            parent: None,
            archive_hash: None,
            device_id: device_id.map(str::to_string),
            created_by: Default::default(),
        }
    }

    #[test]
    fn normalize_heads_migrates_legacy_head_to_last_sync_device() {
        let mut snapshots = GameSnapshots {
            name: "TestGame".into(),
            backups: vec![snapshot("2025-01-01_00-00-00", Some("snapshot-device"))],
            device_heads: HashMap::new(),
            legacy_head: Some("2025-01-01_00-00-00".into()),
            sync_version: 0,
            last_sync_device: Some("remote-device".into()),
            last_sync_timestamp: None,
        };

        snapshots.normalize_heads();

        assert_eq!(
            snapshots
                .head_for_device(&"remote-device".to_string())
                .map(String::as_str),
            Some("2025-01-01_00-00-00")
        );
        assert!(snapshots.legacy_head.is_none());
    }

    #[test]
    fn normalize_heads_removes_heads_pointing_to_missing_snapshots() {
        let mut snapshots = GameSnapshots::new("TestGame");
        snapshots
            .backups
            .push(snapshot("2025-01-01_00-00-00", Some("device-a")));
        snapshots
            .device_heads
            .insert("device-a".into(), "2025-01-01_00-00-00".into());
        snapshots
            .device_heads
            .insert("device-b".into(), "missing-date".into());

        snapshots.normalize_heads();

        assert_eq!(snapshots.device_heads.len(), 1);
        assert_eq!(
            snapshots
                .head_for_device(&"device-a".to_string())
                .map(String::as_str),
            Some("2025-01-01_00-00-00")
        );
        assert!(snapshots.head_for_device(&"device-b".to_string()).is_none());
    }

    #[test]
    fn forgetting_v2_tombstones_clears_only_affected_heads() {
        let mut snapshots = GameSnapshots::new("TestGame");
        snapshots.backups = vec![
            snapshot("deleted", Some("device-a")),
            snapshot("kept", Some("device-b")),
        ];
        snapshots
            .device_heads
            .insert("device-a".into(), "deleted".into());
        snapshots
            .device_heads
            .insert("device-b".into(), "kept".into());

        assert_eq!(
            snapshots.forget_v2_tombstones(&BTreeSet::from(["deleted".into()])),
            1
        );
        assert_eq!(snapshots.backups[0].date, "kept");
        assert!(!snapshots.device_heads.contains_key("device-a"));
        assert_eq!(snapshots.device_heads["device-b"], "kept");
    }
}
