//! Built-in hook that keeps the live scheduler aligned with persisted config.
//!
//! It listens to existing game/config lifecycle events instead of inventing a
//! new hook event taxonomy just for scheduler maintenance.

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::quick_actions::AutoBackupScheduler;
use rgsm_core::hooks::{
    ConfigSavedCtx, GameAddedCtx, GameDeletedCtx, GameUpdatedCtx, LifecycleHook, SchedulerSync,
};

pub struct TauriSchedulerSync {
    app: AppHandle,
}

impl TauriSchedulerSync {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl SchedulerSync for TauriSchedulerSync {
    fn sync_from_config(&self) {
        if let Some(scheduler) = self.app.try_state::<AutoBackupScheduler>() {
            scheduler.sync_from_config();
        }
    }
}

/// Keeps the live auto-backup scheduler synchronized with persisted config changes.
pub struct SchedulerSyncHook {
    scheduler: Arc<dyn SchedulerSync>,
}

impl SchedulerSyncHook {
    pub fn new(scheduler: Arc<dyn SchedulerSync>) -> Self {
        Self { scheduler }
    }
}

#[async_trait]
impl LifecycleHook for SchedulerSyncHook {
    fn name(&self) -> &str {
        "SchedulerSyncHook"
    }

    fn priority(&self) -> u32 {
        40
    }

    async fn on_game_added(&self, _ctx: &GameAddedCtx) -> Result<()> {
        self.scheduler.sync_from_config();
        Ok(())
    }

    async fn on_game_updated(&self, _ctx: &GameUpdatedCtx) -> Result<()> {
        self.scheduler.sync_from_config();
        Ok(())
    }

    async fn on_game_deleted(&self, _ctx: &GameDeletedCtx) -> Result<()> {
        self.scheduler.sync_from_config();
        Ok(())
    }

    async fn on_config_saved(&self, _ctx: &ConfigSavedCtx) -> Result<()> {
        self.scheduler.sync_from_config();
        Ok(())
    }
}
