use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use super::runtime_config_sync::ConfigRuntimeSync;
use crate::quick_actions::AutoBackupScheduler;
use rgsm_core::hooks::{
    ConfigSavedCtx, GameAddedCtx, GameDeletedCtx, GameUpdatedCtx, LifecycleHook,
};

pub struct TauriSchedulerSync {
    app: AppHandle,
}

impl TauriSchedulerSync {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl ConfigRuntimeSync for TauriSchedulerSync {
    async fn sync_from_config(&self) -> Result<()> {
        if let Some(scheduler) = self.app.try_state::<AutoBackupScheduler>() {
            scheduler.sync_from_config();
        }
        Ok(())
    }
}

/// Keeps the live auto-backup scheduler synchronized with persisted config changes.
pub struct SchedulerSyncHook {
    scheduler: Arc<dyn ConfigRuntimeSync>,
}

impl SchedulerSyncHook {
    pub fn new(scheduler: Arc<dyn ConfigRuntimeSync>) -> Self {
        Self { scheduler }
    }

    async fn sync_from_config(&self) -> Result<()> {
        self.scheduler.sync_from_config().await
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
        self.sync_from_config().await
    }

    async fn on_game_updated(&self, _ctx: &GameUpdatedCtx) -> Result<()> {
        self.sync_from_config().await
    }

    async fn on_game_deleted(&self, _ctx: &GameDeletedCtx) -> Result<()> {
        self.sync_from_config().await
    }

    async fn on_config_saved(&self, _ctx: &ConfigSavedCtx) -> Result<()> {
        self.sync_from_config().await
    }
}
