mod archive;
mod capture_plan;
mod catalog;
mod device_binding;
mod extra_backups;
mod extra_info;
mod game;
mod game_snapshots;
mod path_format;
pub(crate) mod registry;
mod restore_plan;
mod restore_targets;
mod save_unit;
mod snapshot;
mod snapshot_capture;
pub(crate) mod state_fingerprint;
pub mod storage_key;
#[cfg(test)]
mod tests;
mod utils;

pub(crate) use archive::ArchiveCaptureGroup;
pub use archive::{
    ArchiveBackend, ArchiveVersion, CompressionPreset, RestoreNotificationLevel, RestoreNotifier,
    SevenZBackend, ZipBackend, archive_file_name, archive_path, remote_archive_path,
    snapshot_archive_path,
};
pub use capture_plan::{
    CaptureGroup, CapturePlan, CapturePlanError, CapturePreflightFailure, CaptureSourceKind,
    SaveUnitCaptureInput,
};
pub use device_binding::{GameDeviceBinding, SavePathOverride};
pub use extra_backups::ExtraBackupItem;
pub use extra_backups::{delete_extra_backup, extra_backup_folder_path, list_extra_backups};
pub use extra_info::{extra_info_dir, extra_info_namespace_dir, extra_info_namespace_file};
pub use game::{
    AutoBackupConfig, Game, GameDraft, LudusaviMeta, StoreGameId, TimerSnapshotDecision,
};
pub use game_snapshots::{GameSnapshots, PendingDescription};
pub use restore_plan::{RestoreEntry, RestorePlan, RestorePlanError};
pub use save_unit::{SaveUnit, SaveUnitDraft, SaveUnitSource, SaveUnitType};
pub use snapshot::{ArchiveFormat, CreatedBy, Snapshot};
pub use state_fingerprint::compute_file_hash;
pub use utils::*;

pub const TIMER_AUTO_BACKUP_DESCRIPTION: &str = "Auto Backup (Timer)";

pub use snapshot_capture::CaptureSnapshotOptions;
