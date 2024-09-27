use serde::{Deserialize, Serialize};

use super::Snapshot;



/// A backup list info is a json file in a backup folder for a game.
/// It contains the name of the game,
/// and all backups' path
#[derive(Debug, Serialize, Deserialize)]
pub struct GameSnapshots {
    pub name: String,
    pub backups: Vec<Snapshot>,
}