//! Built-in hook that keeps the live scheduler aligned with persisted config.
//!
//! It listens to existing game/config lifecycle events instead of inventing a
//! new hook event taxonomy just for scheduler maintenance.

use anyhow::Result;
use async_trait::async_trait;
use tauri::{AppHandle, Manager};

use super::pipeline::{ConfigSavedCtx, GameAddedCtx, GameDeletedCtx, GameUpdatedCtx, SnapshotHook};
use crate::quick_actions::AutoBackupScheduler;

/// Keeps the live auto-backup scheduler synchronized with persisted config changes.
pub struct SchedulerSyncHook {
    app: AppHandle,
}

impl SchedulerSyncHook {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    fn sync_scheduler(&self) {
        if let Some(scheduler) = self.app.try_state::<AutoBackupScheduler>() {
            scheduler.sync_from_config();
        }
    }
}

#[async_trait]
impl SnapshotHook for SchedulerSyncHook {
    fn name(&self) -> &str {
        "SchedulerSyncHook"
    }

    fn priority(&self) -> u32 {
        40
    }

    async fn on_game_added(&self, _ctx: &GameAddedCtx) -> Result<()> {
        self.sync_scheduler();
        Ok(())
    }

    async fn on_game_updated(&self, _ctx: &GameUpdatedCtx) -> Result<()> {
        self.sync_scheduler();
        Ok(())
    }

    async fn on_game_deleted(&self, _ctx: &GameDeletedCtx) -> Result<()> {
        self.sync_scheduler();
        Ok(())
    }

    async fn on_config_saved(&self, _ctx: &ConfigSavedCtx) -> Result<()> {
        self.sync_scheduler();
        Ok(())
    }
}
