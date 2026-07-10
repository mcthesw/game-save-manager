mod archive;
mod device_binding;
mod extra_backups;
mod extra_info;
mod game;
mod game_snapshots;
mod path_format;
pub(crate) mod registry;
mod save_unit;
mod snapshot;
mod state_fingerprint;
pub mod storage_key;
#[cfg(test)]
mod tests;
mod utils;

pub use archive::{
    ArchiveBackend, CompressionPreset, RestoreNotificationLevel, RestoreNotifier, ZipBackend,
};
pub use device_binding::GameDeviceBinding;
pub use extra_backups::ExtraBackupItem;
pub use extra_backups::{
    delete_extra_backup, extra_backup_folder_path, list_extra_backups, restore_extra_backup,
};
pub use extra_info::{extra_info_dir, extra_info_namespace_dir, extra_info_namespace_file};
pub use game::{
    AutoBackupConfig, Game, GameDraft, LudusaviMeta, StoreGameId, TimerSnapshotDecision,
};
pub use game_snapshots::GameSnapshots;
pub use save_unit::{SaveUnit, SaveUnitDraft, SaveUnitType};
pub use snapshot::{CreatedBy, Snapshot};
pub use state_fingerprint::compute_file_hash;
pub use utils::*;

pub const TIMER_AUTO_BACKUP_DESCRIPTION: &str = "Auto Backup (Timer)";
