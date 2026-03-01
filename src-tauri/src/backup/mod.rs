mod archive;
mod extra_backups;
mod game;
mod game_snapshots;
mod path_format;
mod save_unit;
mod snapshot;
mod state_fingerprint;
#[cfg(test)]
mod tests;
mod utils;

pub use archive::{ArchiveBackend, CompressionPreset, ZipBackend};
pub use extra_backups::ExtraBackupItem;
pub use extra_backups::{
    delete_extra_backup, extra_backup_folder_path, list_extra_backups, restore_extra_backup,
};
pub use game::Game;
pub(crate) use game::TimerSnapshotDecision;
pub use game_snapshots::GameSnapshots;
pub use save_unit::{SaveUnit, SaveUnitType};
pub use snapshot::Snapshot;
pub use utils::*;

pub(crate) const TIMER_AUTO_BACKUP_DESCRIPTION: &str = "Auto Backup (Timer)";
