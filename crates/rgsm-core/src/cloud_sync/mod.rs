mod backend;
mod cloud_settings;
#[allow(dead_code)]
pub mod conflict;
mod facade;
mod recovery;
mod state_recording;
pub mod sync_state;
mod task_manager;
pub mod transfer;
mod utils;
pub mod v2;

pub const V1_CONFIG_PATH: &str = "/GameSaveManager.config.json";
pub const V1_SAVE_DATA_PREFIX: &str = "save_data/";

pub use backend::{
    Backend, CloudBackendCheckItem, CloudBackendCheckItemStatus, CloudBackendCheckOutcome,
    CloudBackendCheckReport, CloudBackendCheckStep, CloudSyncSessionConfig, S3AddressingStyle,
};
pub use cloud_settings::CloudSettings;
#[allow(unused_imports)]
pub use conflict::{ConflictResolution, SyncRelation};
pub use facade::{
    BatchSyncItemStatus, BatchSyncReport, SyncGameOutcome, download_all_from_session,
    session_from_backend, sync_game, upload_all_from_session,
};
pub use recovery::{ConflictResolutionOutcome, resolve_game_conflict, sync_config};
#[allow(unused_imports)]
pub use sync_state::{GameSyncState, PendingAction, SyncResult, SyncState};
pub use task_manager::{
    CancelCloudSyncResult, CloudSyncError, CloudSyncJob, CloudSyncJobInfo, CloudSyncJobStatus,
    CloudSyncStatus, CloudSyncTaskManager, SyncEventEmitter,
};
pub use utils::*;
