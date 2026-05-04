//! Built-in hook that keeps quick action runtime state aligned with config.

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use super::runtime_config_sync::ConfigRuntimeSync;
use crate::quick_actions::QuickActionManager;
use rgsm_core::hooks::{ConfigSavedCtx, GameDeletedCtx, GameUpdatedCtx, LifecycleHook};

pub struct TauriQuickActionRuntimeSync {
    app: AppHandle,
}

impl TauriQuickActionRuntimeSync {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl ConfigRuntimeSync for TauriQuickActionRuntimeSync {
    async fn sync_from_config(&self) -> Result<()> {
        if let Some(manager) = self.app.try_state::<Arc<QuickActionManager>>() {
            manager.reload_current_game_from_config().await?;
        }
        Ok(())
    }
}

/// Refreshes the live quick action manager after persisted config changes.
pub struct QuickActionSyncHook {
    runtime: Arc<dyn ConfigRuntimeSync>,
}

impl QuickActionSyncHook {
    pub fn new(runtime: Arc<dyn ConfigRuntimeSync>) -> Self {
        Self { runtime }
    }

    async fn sync_from_config(&self) -> Result<()> {
        self.runtime.sync_from_config().await
    }
}

#[async_trait]
impl LifecycleHook for QuickActionSyncHook {
    fn name(&self) -> &str {
        "QuickActionSyncHook"
    }

    fn priority(&self) -> u32 {
        45
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

#[cfg(test)]
mod tests {
    use super::*;
    use rgsm_core::config::Config;
    use rgsm_core::hooks::{ConfigSavedCtx, HookSource};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingRuntimeSync {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl ConfigRuntimeSync for CountingRuntimeSync {
        async fn sync_from_config(&self) -> Result<()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn config_saved_refreshes_quick_action_runtime() {
        let sync = Arc::new(CountingRuntimeSync {
            calls: AtomicUsize::new(0),
        });
        let hook = QuickActionSyncHook::new(sync.clone());

        hook.on_config_saved(&ConfigSavedCtx {
            config: Config::default(),
            source: HookSource::UserManual,
        })
        .await
        .expect("quick action sync should succeed");

        assert_eq!(sync.calls.load(Ordering::SeqCst), 1);
    }
}
