mod archive;
mod extra_backups;
mod game;
mod game_snapshots;
mod path_format;
pub(crate) mod registry;
mod scanner;
mod save_unit;
mod snapshot;
mod state_fingerprint;
#[cfg(test)]
mod tests;
mod utils;

pub use archive::{
    ArchiveBackend, CompressionPreset, RestoreNotificationLevel, RestoreNotifier, ZipBackend,
};
pub use extra_backups::ExtraBackupItem;
pub use extra_backups::{
    delete_extra_backup, extra_backup_folder_path, list_extra_backups, restore_extra_backup,
};
pub use game::{AutoBackupConfig, Game, GameDraft, TimerSnapshotDecision};
pub use game_snapshots::GameSnapshots;
pub use save_unit::{SaveUnit, SaveUnitDraft, SaveUnitType};
pub use scanner::scan_directories;
pub use snapshot::{CreatedBy, Snapshot};
pub use state_fingerprint::compute_file_hash;
pub use utils::*;

pub const TIMER_AUTO_BACKUP_DESCRIPTION: &str = "Auto Backup (Timer)";
