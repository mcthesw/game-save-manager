use serde::{Deserialize, Serialize};
use specta::Type;

use super::Snapshot;

/// A backup list info is a json file in a backup folder for a game.
/// It contains the name of the game,
/// and all backups' path
#[derive(Debug, Serialize, Deserialize, Type)]
pub struct GameSnapshots {
    pub name: String,
    pub backups: Vec<Snapshot>,
}
