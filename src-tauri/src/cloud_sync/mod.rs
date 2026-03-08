mod backend;
mod cloud_settings;
#[allow(dead_code)]
pub mod conflict;
mod facade;
pub mod sync_state;
mod task_manager;
pub mod transfer;
mod utils;

pub use backend::{Backend, CloudSyncSessionConfig};
pub use cloud_settings::CloudSettings;
#[allow(unused_imports)]
pub use conflict::{ConflictResolution, SyncRelation};
pub use facade::{
    BatchSyncItemStatus, BatchSyncReport, SyncGameOutcome, download_all_from_session,
    session_from_backend, sync_game_from_config, upload_all_from_session,
};
#[allow(unused_imports)]
pub use sync_state::{GameSyncState, PendingAction, SyncResult, SyncState};
pub use task_manager::{
    CancelCloudSyncResult, CloudSyncError, CloudSyncJob, CloudSyncJobStatus, CloudSyncStatus,
    CloudSyncTaskManager,
};
pub use utils::*;
