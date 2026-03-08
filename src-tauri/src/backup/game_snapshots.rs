use serde::{Deserialize, Serialize};
use specta::Type;

use super::Snapshot;
use crate::default_value;
use crate::device::DeviceId;

/// A backup list info is a json file in a backup folder for a game.
/// It contains the name of the game,
/// and all backups' path
#[derive(Debug, Serialize, Deserialize, Type, Clone)]
pub struct GameSnapshots {
    pub name: String,
    pub backups: Vec<Snapshot>,
    /// HEAD points to the current snapshot that new snapshots will branch from.
    /// If None, new snapshots will be created as root nodes.
    #[serde(default = "default_value::default_none")]
    pub head: Option<String>,
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
