use std::path::PathBuf;

use anyhow::Result as HookResult;
use async_trait::async_trait;
use log::{error, info};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::backup::{Game, GameSnapshots, Snapshot};
use crate::config::Config;
use crate::preclude::BackupError;

// ── HookSource ──────────────────────────────────────────────────────────────

/// Describes *why* the snapshot lifecycle event was triggered,
/// allowing downstream hooks to adjust behaviour (e.g. play sounds
/// only for quick-action / timer sources).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum HookSource {
    /// User clicked a button or menu item.
    UserManual,
    /// Periodic timer-based auto-backup.
    TimerAutoBackup,
    /// Batch backup/apply-all operation.
    BatchOperation,
    /// Global hotkey shortcut.
    QuickActionHotkey,
    /// Tray-menu shortcut.
    QuickActionTray,
    /// Cloud-sync subsystem itself (e.g. download-then-apply).
    CloudSync,
    /// System-internal operations (migration, cleanup, etc.).
    Internal,
}

// ── Context structs ─────────────────────────────────────────────────────────

/// Passed after a new snapshot archive has been written to disk.
pub struct SnapshotCreatedCtx {
    pub config: Config,
    pub source: HookSource,
    pub game: Game,
    pub snapshot: Snapshot,
    pub snapshots: GameSnapshots,
    pub local_zip_path: PathBuf,
    pub remote_zip_path: String,
}

/// Passed after a snapshot archive has been deleted from disk.
pub struct SnapshotDeletedCtx {
    pub config: Config,
    pub source: HookSource,
    pub game: Game,
    pub snapshots: GameSnapshots,
    /// Remote paths that were deleted (may be empty).
    pub deleted_remote_paths: Vec<String>,
}

/// Passed after a snapshot has been restored / applied to the game folder.
#[allow(dead_code)]
pub struct SnapshotAppliedCtx {
    pub config: Config,
    pub source: HookSource,
    pub game: Game,
    pub snapshot: Snapshot,
    pub snapshots: GameSnapshots,
}

/// Passed **before** a snapshot is about to be restored.
///
/// This is a **gate event**: if any hook returns `Err`, the restore is
/// aborted and the error propagated to the caller.  Hooks that perform
/// non-critical work (e.g. extra backup) should handle their own errors
/// internally and return `Ok(())`.
#[allow(dead_code)]
pub struct BeforeRestoreCtx {
    pub config: Config,
    pub source: HookSource,
    pub game: Game,
    pub snapshot: Snapshot,
    pub snapshots: GameSnapshots,
    /// Path to the archive that is about to be decompressed.
    pub archive_path: PathBuf,
}

/// Passed after snapshot metadata has been modified (description, HEAD, parent).
pub struct MetadataChangedCtx {
    pub config: Config,
    pub source: HookSource,
    pub game: Game,
    pub snapshots: GameSnapshots,
}

/// Passed after a new game has been added to the config.
pub struct GameAddedCtx {
    pub config: Config,
    pub source: HookSource,
    pub game: Game,
    pub snapshots: GameSnapshots,
}

/// Passed after an existing game has been updated in the config.
#[allow(dead_code)]
pub struct GameUpdatedCtx {
    pub config: Config,
    pub source: HookSource,
    pub previous_game: Game,
    pub game: Game,
}

/// Passed after a game (and all its snapshots) has been deleted.
pub struct GameDeletedCtx {
    pub config: Config,
    pub source: HookSource,
    pub game_name: String,
    pub remote_game_dir_path: String,
}

/// Passed after config.json has been saved to disk.
#[allow(dead_code)]
pub struct ConfigSavedCtx {
    pub config: Config,
    pub source: HookSource,
}

/// Passed after a sync operation finished (success or error).
#[allow(dead_code)]
pub struct SyncCompletedCtx {
    pub config: Config,
    pub source: HookSource,
    pub game_name: String,
    pub success: bool,
    pub message: Option<String>,
}

/// Passed when a sync conflict is detected.
#[allow(dead_code)]
pub struct SyncConflictCtx {
    pub config: Config,
    pub source: HookSource,
    pub game_name: String,
    pub local_head: Option<String>,
    pub remote_head: Option<String>,
}

// ── SnapshotHook trait ──────────────────────────────────────────────────────

/// Extension point for snapshot-lifecycle side effects.
///
/// All methods have default no-op implementations so concrete hooks
/// only need to override the events they care about.
///
/// **Ordering**: hooks are executed in ascending `priority()` order.
/// Lower numbers run first (e.g. Checksum=10 → CloudSync=50 → Notification=90).
#[async_trait]
pub trait SnapshotHook: Send + Sync {
    /// Human-readable name shown in logs.
    fn name(&self) -> &str;

    /// Lower runs first.  Avoid collisions with the built-in range 0..100.
    fn priority(&self) -> u32 {
        100
    }

    async fn on_snapshot_created(&self, _ctx: &mut SnapshotCreatedCtx) -> HookResult<()> {
        Ok(())
    }
    async fn on_snapshot_deleted(&self, _ctx: &SnapshotDeletedCtx) -> HookResult<()> {
        Ok(())
    }
    /// **Gate hook** — returning `Err` aborts the restore operation.
    async fn on_before_restore(&self, _ctx: &BeforeRestoreCtx) -> Result<(), BackupError> {
        Ok(())
    }
    async fn on_snapshot_applied(&self, _ctx: &SnapshotAppliedCtx) -> HookResult<()> {
        Ok(())
    }
    async fn on_metadata_changed(&self, _ctx: &MetadataChangedCtx) -> HookResult<()> {
        Ok(())
    }
    async fn on_game_added(&self, _ctx: &GameAddedCtx) -> HookResult<()> {
        Ok(())
    }
    async fn on_game_updated(&self, _ctx: &GameUpdatedCtx) -> HookResult<()> {
        Ok(())
    }
    async fn on_game_deleted(&self, _ctx: &GameDeletedCtx) -> HookResult<()> {
        Ok(())
    }
    async fn on_config_saved(&self, _ctx: &ConfigSavedCtx) -> HookResult<()> {
        Ok(())
    }
    async fn on_sync_completed(&self, _ctx: &SyncCompletedCtx) -> HookResult<()> {
        Ok(())
    }
    async fn on_sync_conflict(&self, _ctx: &SyncConflictCtx) -> HookResult<()> {
        Ok(())
    }
}

// ── HookPipeline ────────────────────────────────────────────────────────────

/// Owns a priority-sorted list of hooks and fans out every event to each
/// hook in order.  Individual hook errors are **logged but never abort**
/// subsequent hooks.
pub struct HookPipeline {
    hooks: Vec<Box<dyn SnapshotHook>>,
}

impl HookPipeline {
    pub fn new(mut hooks: Vec<Box<dyn SnapshotHook>>) -> Self {
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
    use std::sync::{Arc, Mutex};

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
                save_paths: vec![],
                game_paths: Default::default(),
                cloud_sync_enabled: true,
                next_save_unit_id: 0,
            },
            snapshot: Snapshot {
                date: "2025-01-01T00:00:00".into(),
                describe: String::new(),
                path: "/tmp/test.zip".into(),
                size: 0,
                parent: None,
                archive_hash: None,
                device_id: None,
            },
            snapshots: GameSnapshots::new("TestGame"),
            local_zip_path: PathBuf::from("/tmp/test.zip"),
            remote_zip_path: "TestGame/2025-01-01T00:00:00.zip".into(),
        }
    }

    fn make_game_added_ctx() -> GameAddedCtx {
        GameAddedCtx {
            config: Config::default(),
            source: HookSource::UserManual,
            game: Game {
                name: "NewGame".into(),
                save_paths: vec![],
                game_paths: Default::default(),
                cloud_sync_enabled: true,
                next_save_unit_id: 0,
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
                save_paths: vec![],
                game_paths: Default::default(),
                cloud_sync_enabled: false,
                next_save_unit_id: 0,
            },
            game: Game {
                name: "ExistingGame".into(),
                save_paths: vec![],
                game_paths: Default::default(),
                cloud_sync_enabled: true,
                next_save_unit_id: 0,
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
        assert_ne!(HookSource::QuickActionTray, HookSource::QuickActionHotkey);
        assert_eq!(HookSource::Internal, HookSource::Internal);
    }

    fn make_before_restore_ctx() -> BeforeRestoreCtx {
        BeforeRestoreCtx {
            config: Config::default(),
            source: HookSource::UserManual,
            game: Game {
                name: "TestGame".into(),
                save_paths: vec![],
                game_paths: Default::default(),
                cloud_sync_enabled: true,
                next_save_unit_id: 0,
            },
            snapshot: Snapshot {
                date: "2025-01-01T00:00:00".into(),
                describe: String::new(),
                path: "/tmp/test.zip".into(),
                size: 0,
                parent: None,
                archive_hash: Some("abc123".into()),
                device_id: None,
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
