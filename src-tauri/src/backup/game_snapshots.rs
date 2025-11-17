use serde::{Deserialize, Serialize};
use specta::Type;

use super::Snapshot;
use crate::default_value;

/// A backup list info is a json file in a backup folder for a game.
/// It contains the name of the game,
/// and all backups' path
#[derive(Debug, Serialize, Deserialize, Type)]
pub struct GameSnapshots {
    pub name: String,
    pub backups: Vec<Snapshot>,
    /// The current HEAD snapshot (date). Used for creating new snapshots in tree mode.
    /// If None, the latest snapshot is considered HEAD.
    #[serde(default = "default_value::default_none")]
    pub current_head: Option<String>,
}
