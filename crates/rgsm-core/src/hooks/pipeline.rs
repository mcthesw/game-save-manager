//! Ordered dispatch for lifecycle hooks.
//!
//! Contexts and traits live in sibling modules; this file owns the
//! priority-sorted dispatcher.

pub use super::contexts::*;
pub use super::traits::{HookResult, LifecycleHook, SnapshotHook};
use crate::preclude::BackupError;
use log::{error, info};

/// Owns a priority-sorted list of hooks and fans out every event to each
/// hook in order.  Individual hook errors are **logged but never abort**
/// subsequent hooks.
pub struct HookPipeline {
    hooks: Vec<Box<dyn LifecycleHook>>,
}

impl HookPipeline {
    pub fn new(mut hooks: Vec<Box<dyn LifecycleHook>>) -> Self {
        hooks.sort_by_key(|h| h.priority());
        info!(
            target: "rgsm::hooks",
            "HookPipeline initialised with {} hooks: [{}]",
            hooks.len(),
            hooks
                .iter()
                .map(|h| format!("{}({})", h.name(), h.priority()))
                .collect::<Vec<_>>()
                .join(", ")
        );
        Self { hooks }
    }
}

// Macro to reduce boilerplate for fire_* methods (notify — errors logged, never abort).
macro_rules! fire {
    ($self:ident, $method:ident, $ctx:expr) => {{
        for hook in &$self.hooks {
            if let Err(e) = hook.$method($ctx).await {
                error!(
                    target: "rgsm::hooks",
                    "Hook '{}' failed in {}: {e:#}",
                    hook.name(),
                    stringify!($method)
                );
            }
        }
    }};
}

// Gate macro — aborts on first hook error and propagates it.
macro_rules! gate {
    ($self:ident, $method:ident, $ctx:expr) => {{
        for hook in &$self.hooks {
            if let Err(e) = hook.$method($ctx).await {
                error!(
                    target: "rgsm::hooks",
                    "Gate hook '{}' aborted {}: {e:#}",
                    hook.name(),
                    stringify!($method)
                );
                return Err(e);
            }
        }
        Ok(())
    }};
}

impl HookPipeline {
    pub async fn fire_snapshot_created(&self, ctx: &mut SnapshotCreatedCtx) {
        fire!(self, on_snapshot_created, ctx);
    }
    pub async fn fire_snapshot_deleted(&self, ctx: &SnapshotDeletedCtx) {
        fire!(self, on_snapshot_deleted, ctx);
    }
    /// Gate: aborts on first hook error, returning it to the caller.
    pub async fn fire_before_restore(&self, ctx: &BeforeRestoreCtx) -> Result<(), BackupError> {
        gate!(self, on_before_restore, ctx)
    }
    pub async fn fire_snapshot_applied(&self, ctx: &SnapshotAppliedCtx) {
        fire!(self, on_snapshot_applied, ctx);
    }
    pub async fn fire_metadata_changed(&self, ctx: &MetadataChangedCtx) {
        fire!(self, on_metadata_changed, ctx);
    }
    pub async fn fire_game_added(&self, ctx: &GameAddedCtx) {
        fire!(self, on_game_added, ctx);
    }
    pub async fn fire_game_updated(&self, ctx: &GameUpdatedCtx) {
        fire!(self, on_game_updated, ctx);
    }
    pub async fn fire_game_deleted(&self, ctx: &GameDeletedCtx) {
        fire!(self, on_game_deleted, ctx);
    }
    pub async fn fire_config_saved(&self, ctx: &ConfigSavedCtx) {
        fire!(self, on_config_saved, ctx);
    }
    #[allow(dead_code)]
    pub async fn fire_sync_completed(&self, ctx: &SyncCompletedCtx) {
        fire!(self, on_sync_completed, ctx);
    }
    #[allow(dead_code)]
    pub async fn fire_sync_conflict(&self, ctx: &SyncConflictCtx) {
        fire!(self, on_sync_conflict, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use crate::backup::{Game, GameSnapshots, Snapshot};
    use crate::config::Config;

    /// A test hook that records which events were fired and in what order.
    struct RecorderHook {
        name: String,
        prio: u32,
        log: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl SnapshotHook for RecorderHook {
        fn name(&self) -> &str {
            &self.name
        }
        fn priority(&self) -> u32 {
            self.prio
        }
        async fn on_snapshot_created(&self, _ctx: &mut SnapshotCreatedCtx) -> HookResult<()> {
            self.log
                .lock()
                .unwrap()
                .push(format!("{}:created", self.name));
            Ok(())
        }
        async fn on_snapshot_deleted(&self, _ctx: &SnapshotDeletedCtx) -> HookResult<()> {
            self.log
                .lock()
                .unwrap()
                .push(format!("{}:deleted", self.name));
            Ok(())
        }
        async fn on_game_added(&self, _ctx: &GameAddedCtx) -> HookResult<()> {
            self.log
                .lock()
                .unwrap()
                .push(format!("{}:game_added", self.name));
            Ok(())
        }
        async fn on_game_updated(&self, _ctx: &GameUpdatedCtx) -> HookResult<()> {
            self.log
                .lock()
                .unwrap()
                .push(format!("{}:game_updated", self.name));
            Ok(())
        }
        async fn on_before_restore(&self, _ctx: &BeforeRestoreCtx) -> Result<(), BackupError> {
            self.log
                .lock()
                .unwrap()
                .push(format!("{}:before_restore", self.name));
            Ok(())
        }
    }

    /// A hook that always fails, to verify error isolation.
    struct FailingHook {
        prio: u32,
        log: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl SnapshotHook for FailingHook {
        fn name(&self) -> &str {
            "failing"
        }
        fn priority(&self) -> u32 {
            self.prio
        }
        async fn on_snapshot_created(&self, _ctx: &mut SnapshotCreatedCtx) -> HookResult<()> {
            self.log.lock().unwrap().push("failing:created".to_string());
            anyhow::bail!("intentional test failure")
        }
        async fn on_before_restore(&self, _ctx: &BeforeRestoreCtx) -> Result<(), BackupError> {
            self.log
                .lock()
                .unwrap()
                .push("failing:before_restore".to_string());
            Err(BackupError::Unexpected(anyhow::anyhow!(
                "intentional gate failure"
            )))
        }
    }

    fn make_snapshot_created_ctx() -> SnapshotCreatedCtx {
        SnapshotCreatedCtx {
            config: Config::default(),
            source: HookSource::UserManual,
            game: Game {
                name: "TestGame".into(),
                storage_key: String::new(),
                save_paths: vec![],
                game_paths: Default::default(),
                cloud_sync_enabled: true,
                auto_backup: None,
                next_save_unit_id: 0,
                ludusavi_meta: None,
                device_bindings: std::collections::HashMap::new(),
            },
            snapshot: Snapshot {
                date: "2025-01-01T00:00:00".into(),
                describe: String::new(),
                path: "/tmp/test.zip".into(),
                archive_format: crate::backup::ArchiveFormat::Zip,
                size: 0,
                parent: None,
                archive_hash: None,
                device_id: None,
                created_by: Default::default(),
            },
            snapshots: GameSnapshots::new("TestGame"),
            local_archive_path: PathBuf::from("/tmp/test.zip"),
            remote_archive_path: "TestGame/2025-01-01T00:00:00.zip".into(),
        }
    }

    fn make_game_added_ctx() -> GameAddedCtx {
        GameAddedCtx {
            config: Config::default(),
            source: HookSource::UserManual,
            game: Game {
                name: "NewGame".into(),
                storage_key: String::new(),
                save_paths: vec![],
                game_paths: Default::default(),
                cloud_sync_enabled: true,
                auto_backup: None,
                next_save_unit_id: 0,
                ludusavi_meta: None,
                device_bindings: std::collections::HashMap::new(),
            },
            snapshots: GameSnapshots::new("NewGame"),
        }
    }

    fn make_game_updated_ctx() -> GameUpdatedCtx {
        GameUpdatedCtx {
            config: Config::default(),
            source: HookSource::UserManual,
            previous_game: Game {
                name: "ExistingGame".into(),
                storage_key: String::new(),
                save_paths: vec![],
                game_paths: Default::default(),
                cloud_sync_enabled: false,
                auto_backup: None,
                next_save_unit_id: 0,
                ludusavi_meta: None,
                device_bindings: std::collections::HashMap::new(),
            },
            game: Game {
                name: "ExistingGame".into(),
                storage_key: String::new(),
                save_paths: vec![],
                game_paths: Default::default(),
                cloud_sync_enabled: true,
                auto_backup: None,
                next_save_unit_id: 0,
                ludusavi_meta: None,
                device_bindings: std::collections::HashMap::new(),
            },
        }
    }

    #[tokio::test]
    async fn pipeline_executes_hooks_in_priority_order() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let pipeline = HookPipeline::new(vec![
            Box::new(RecorderHook {
                name: "C".into(),
                prio: 90,
                log: log.clone(),
            }),
            Box::new(RecorderHook {
                name: "A".into(),
                prio: 10,
                log: log.clone(),
            }),
            Box::new(RecorderHook {
                name: "B".into(),
                prio: 50,
                log: log.clone(),
            }),
        ]);

        let mut ctx = make_snapshot_created_ctx();
        pipeline.fire_snapshot_created(&mut ctx).await;

        let entries = log.lock().unwrap();
        assert_eq!(*entries, vec!["A:created", "B:created", "C:created"]);
    }

    #[tokio::test]
    async fn pipeline_continues_after_hook_failure() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let pipeline = HookPipeline::new(vec![
            Box::new(RecorderHook {
                name: "before".into(),
                prio: 10,
                log: log.clone(),
            }),
            Box::new(FailingHook {
                prio: 50,
                log: log.clone(),
            }),
            Box::new(RecorderHook {
                name: "after".into(),
                prio: 90,
                log: log.clone(),
            }),
        ]);

        let mut ctx = make_snapshot_created_ctx();
        pipeline.fire_snapshot_created(&mut ctx).await;

        let entries = log.lock().unwrap();
        assert_eq!(
            *entries,
            vec!["before:created", "failing:created", "after:created"]
        );
    }

    #[tokio::test]
    async fn pipeline_with_no_hooks_does_not_panic() {
        let pipeline = HookPipeline::new(vec![]);
        let mut ctx = make_snapshot_created_ctx();
        pipeline.fire_snapshot_created(&mut ctx).await;
        pipeline.fire_game_added(&make_game_added_ctx()).await;
    }

    #[tokio::test]
    async fn pipeline_fires_different_events_independently() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let pipeline = HookPipeline::new(vec![Box::new(RecorderHook {
            name: "r".into(),
            prio: 10,
            log: log.clone(),
        })]);

        let mut ctx = make_snapshot_created_ctx();
        pipeline.fire_snapshot_created(&mut ctx).await;
        pipeline.fire_game_added(&make_game_added_ctx()).await;
        pipeline.fire_game_updated(&make_game_updated_ctx()).await;

        let entries = log.lock().unwrap();
        assert_eq!(
            *entries,
            vec!["r:created", "r:game_added", "r:game_updated"]
        );
    }

    #[test]
    fn hook_source_variants_are_distinct() {
        assert_ne!(HookSource::UserManual, HookSource::TimerAutoBackup);
        assert_ne!(HookSource::QuickActionHotkey, HookSource::CloudSync);
        assert_ne!(HookSource::CloudSync, HookSource::CloudConflictResolution);
        assert_ne!(HookSource::QuickActionTray, HookSource::QuickActionHotkey);
        assert_eq!(HookSource::Internal, HookSource::Internal);
    }

    fn make_before_restore_ctx() -> BeforeRestoreCtx {
        BeforeRestoreCtx {
            capture_plan: None,
            config: Config::default(),
            source: HookSource::UserManual,
            game: Game {
                name: "TestGame".into(),
                storage_key: String::new(),
                save_paths: vec![],
                game_paths: Default::default(),
                cloud_sync_enabled: true,
                auto_backup: None,
                next_save_unit_id: 0,
                ludusavi_meta: None,
                device_bindings: std::collections::HashMap::new(),
            },
            snapshot: Snapshot {
                date: "2025-01-01T00:00:00".into(),
                describe: String::new(),
                path: "/tmp/test.zip".into(),
                archive_format: crate::backup::ArchiveFormat::Zip,
                size: 0,
                parent: None,
                archive_hash: Some("abc123".into()),
                device_id: None,
                created_by: Default::default(),
            },
            snapshots: GameSnapshots::new("TestGame"),
            archive_path: PathBuf::from("/tmp/test.zip"),
        }
    }

    #[tokio::test]
    async fn gate_hook_aborts_on_first_error() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let pipeline = HookPipeline::new(vec![
            Box::new(RecorderHook {
                name: "pre".into(),
                prio: 5,
                log: log.clone(),
            }),
            Box::new(FailingHook {
                prio: 10,
                log: log.clone(),
            }),
            Box::new(RecorderHook {
                name: "post".into(),
                prio: 90,
                log: log.clone(),
            }),
        ]);

        let result = pipeline
            .fire_before_restore(&make_before_restore_ctx())
            .await;

        assert!(result.is_err());
        let entries = log.lock().unwrap();
        // "post" should NOT have run — gate aborted after "failing"
        assert_eq!(
            *entries,
            vec!["pre:before_restore", "failing:before_restore"]
        );
    }

    #[tokio::test]
    async fn gate_hook_succeeds_when_all_pass() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let pipeline = HookPipeline::new(vec![
            Box::new(RecorderHook {
                name: "a".into(),
                prio: 5,
                log: log.clone(),
            }),
            Box::new(RecorderHook {
                name: "b".into(),
                prio: 50,
                log: log.clone(),
            }),
        ]);

        let result = pipeline
            .fire_before_restore(&make_before_restore_ctx())
            .await;

        assert!(result.is_ok());
        let entries = log.lock().unwrap();
        assert_eq!(*entries, vec!["a:before_restore", "b:before_restore"]);
    }
}
