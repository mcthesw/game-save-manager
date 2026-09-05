mod cloud_archive;
mod cloud_library_metadata;
mod cloud_library_target;
mod config;
mod conflict_resolution;
mod device_game_lifecycle;
mod game;
mod game_deletion;
mod path_resolution;
mod profile_management;
mod retention;
mod snapshot;
mod snapshot_sync;
mod sync;

use std::sync::Arc;

use crate::hooks::HookPipeline;

pub use conflict_resolution::AcceptRemoteProgressOutcome;
pub use device_game_lifecycle::DeviceGameStatus;
pub use game_deletion::DeletedCloudGameView;
pub use profile_management::CloudDeviceProfileView;
pub use retention::SnapshotRetentionOutcome;
pub use snapshot_sync::{
    DEFAULT_SNAPSHOT_SYNC_POLL_MINUTES, LiveSaveApplyPlan, LiveSaveSyncTarget,
    SnapshotSyncServiceError, build_v2_snapshot_sync_hook, resume_v2_snapshot_sync,
    review_v2_live_save_apply, run_v2_snapshot_sync_once, v2_live_save_sync_targets,
    v2_snapshot_sync_poll_minutes,
};
pub use sync::{
    CloudLibraryCutoverOutcome, CloudLibraryJoinOutcome, CloudLibraryServiceError,
    CloudLibraryStatus, CurrentPositionDecision, GameSyncModeOutcome, LiveSaveSyncOptions,
};

#[derive(Clone)]
pub struct ServiceContext {
    pipeline: Arc<HookPipeline>,
}

impl ServiceContext {
    pub fn new(pipeline: Arc<HookPipeline>) -> Self {
        Self { pipeline }
    }

    pub fn pipeline(&self) -> &HookPipeline {
        self.pipeline.as_ref()
    }
}
