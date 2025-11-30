use serde::{Deserialize, Serialize};
use specta::Type;

use super::Snapshot;
use crate::default_value;

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
}
