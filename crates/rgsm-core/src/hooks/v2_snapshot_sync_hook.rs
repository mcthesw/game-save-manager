use super::{HookSource, LifecycleHook, SnapshotAppliedCtx, SnapshotCreatedCtx};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotSyncTarget {
    pub activation_revision: u64,
    pub local_baseline: BTreeSet<String>,
    pub retention_limit: Option<u32>,
    pub upload_new_archives: bool,
}

/// Application-supplied wakeup only: no network I/O belongs in a local operation hook.
pub struct V2SnapshotSyncHook {
    request: Arc<dyn Fn() + Send + Sync>,
}

impl V2SnapshotSyncHook {
    pub(crate) fn new(request: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self { request }
    }

    fn changed(&self, source: &HookSource, game: &crate::backup::Game) {
        if *source != HookSource::CloudSync && game.cloud_sync_enabled {
            (self.request)();
        }
    }
}

#[async_trait]
impl LifecycleHook for V2SnapshotSyncHook {
    fn name(&self) -> &str {
        "V2SnapshotSyncHook"
    }
    fn priority(&self) -> u32 {
        50
    }
    async fn on_snapshot_committed(&self, ctx: &SnapshotCreatedCtx) -> Result<()> {
        self.changed(&ctx.source, &ctx.game);
        Ok(())
    }
    async fn on_snapshot_applied(&self, ctx: &SnapshotAppliedCtx) -> Result<()> {
        self.changed(&ctx.source, &ctx.game);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn local_changes_only_signal_and_cloud_changes_do_not_loop() {
        let requests = Arc::new(AtomicUsize::new(0));
        let recorder = requests.clone();
        let hook = V2SnapshotSyncHook::new(Arc::new(move || {
            recorder.fetch_add(1, Ordering::SeqCst);
        }));
        let mut game: crate::backup::Game = serde_json::from_value(
            serde_json::json!({"name":"game", "storage_key":"game", "save_paths":[]}),
        )
        .unwrap();
        hook.changed(&HookSource::UserManual, &game);
        hook.changed(&HookSource::CloudSync, &game);
        game.storage_key = "new-game".into();
        hook.changed(&HookSource::UserManual, &game);
        game.cloud_sync_enabled = false;
        hook.changed(&HookSource::UserManual, &game);
        assert_eq!(requests.load(Ordering::SeqCst), 2);
    }
}
