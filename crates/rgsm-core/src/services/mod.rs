mod config;
mod game;
mod path_resolution;
mod snapshot;
mod snapshot_sync;
mod sync;

use std::sync::Arc;

use crate::hooks::HookPipeline;

pub use snapshot_sync::{
    DEFAULT_SNAPSHOT_SYNC_POLL_MINUTES, SnapshotSyncServiceError, build_v2_snapshot_sync_hook,
    resume_v2_snapshot_sync, run_v2_snapshot_sync_once, v2_snapshot_sync_poll_minutes,
};
pub use sync::{
    CloudLibraryCutoverOutcome, CloudLibraryJoinOutcome, CloudLibraryServiceError,
    CloudLibraryStatus, GameSyncModeOutcome,
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
