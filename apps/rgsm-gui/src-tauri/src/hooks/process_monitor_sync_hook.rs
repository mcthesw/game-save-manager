use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use super::runtime_config_sync::ConfigRuntimeSync;
use crate::quick_actions::ProcessMonitor;
use rgsm_core::hooks::{
    ConfigSavedCtx, GameAddedCtx, GameDeletedCtx, GameUpdatedCtx, LifecycleHook,
};

pub struct TauriProcessMonitorSync {
    app: AppHandle,
}

impl TauriProcessMonitorSync {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl ConfigRuntimeSync for TauriProcessMonitorSync {
    async fn sync_from_config(&self) -> Result<()> {
        if let Some(monitor) = self.app.try_state::<ProcessMonitor>() {
            monitor.sync_from_config();
        }
        Ok(())
    }
}

pub struct ProcessMonitorSyncHook {
    monitor: Arc<dyn ConfigRuntimeSync>,
}

impl ProcessMonitorSyncHook {
    pub fn new(monitor: Arc<dyn ConfigRuntimeSync>) -> Self {
        Self { monitor }
    }

    async fn sync_from_config(&self) -> Result<()> {
        self.monitor.sync_from_config().await
    }
}

#[async_trait]
impl LifecycleHook for ProcessMonitorSyncHook {
    fn name(&self) -> &str {
        "ProcessMonitorSyncHook"
    }

    fn priority(&self) -> u32 {
        42
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
