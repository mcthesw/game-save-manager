//! Keeps the live auto-backup scheduler, process monitor and quick action
//! runtime aligned with persisted configuration changes.

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::quick_actions::{AutoBackupScheduler, ProcessMonitor, QuickActionManager};
use rgsm_core::hooks::{
    ConfigSavedCtx, GameAddedCtx, GameDeletedCtx, GameUpdatedCtx, LifecycleHook,
};

pub struct RuntimeSyncHook {
    app: AppHandle,
}

impl RuntimeSyncHook {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    fn sync_timers(&self) {
        if let Some(scheduler) = self.app.try_state::<AutoBackupScheduler>() {
            scheduler.sync_from_config();
        }
        if let Some(monitor) = self.app.try_state::<ProcessMonitor>() {
            monitor.sync_from_config();
        }
    }

    async fn sync_quick_actions(&self) -> Result<()> {
        if let Err(error) = crate::quick_actions::refresh_hotkeys(&self.app) {
            // A hotkey that another program already owns must not fail the save.
            log::warn!(
                target: "rgsm::hooks::runtime_sync",
                "Could not refresh quick action hotkeys: {error:#}"
            );
        }
        if let Some(manager) = self.app.try_state::<Arc<QuickActionManager>>() {
            manager.reload_current_game_from_config().await?;
        }
        Ok(())
    }

    async fn sync_all(&self) -> Result<()> {
        self.sync_timers();
        self.sync_quick_actions().await
    }
}

#[async_trait]
impl LifecycleHook for RuntimeSyncHook {
    fn name(&self) -> &str {
        "RuntimeSyncHook"
    }

    fn priority(&self) -> u32 {
        40
    }

    // A new game has no hotkey or current-game state to refresh.
    async fn on_game_added(&self, _ctx: &GameAddedCtx) -> Result<()> {
        self.sync_timers();
        Ok(())
    }

    async fn on_game_updated(&self, _ctx: &GameUpdatedCtx) -> Result<()> {
        self.sync_all().await
    }

    async fn on_game_deleted(&self, _ctx: &GameDeletedCtx) -> Result<()> {
        self.sync_all().await
    }

    async fn on_config_saved(&self, _ctx: &ConfigSavedCtx) -> Result<()> {
        self.sync_all().await
    }
}
