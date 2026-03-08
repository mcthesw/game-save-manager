use serde::{Deserialize, Serialize};
use specta::Type;

use crate::default_value;
use crate::device::DeviceId;

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
}
