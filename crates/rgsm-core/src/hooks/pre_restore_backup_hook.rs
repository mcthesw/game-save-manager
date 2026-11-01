//! Built-in gate hook that creates a safety backup before restore.
//!
//! Because it participates in `before_restore`, a failure here aborts the
//! restore: the live save is about to be overwritten and has no other copy.

use async_trait::async_trait;
use log::info;

use super::pipeline::{BeforeRestoreCtx, LifecycleHook};
use crate::preclude::BackupError;

/// Creates an extra (overwrite) backup before a snapshot is restored.
///
/// Priority 20 — runs after the integrity check (15). A corrupt archive must
/// be rejected before this hook creates a backup, because the backup's
/// retention cleanup could evict the only older extra backup.
///
/// A missing capture plan means there is no save data to protect and the
/// hook does nothing. Any other failure aborts the restore.
pub struct PreRestoreBackupHook;

#[async_trait]
impl LifecycleHook for PreRestoreBackupHook {
    fn name(&self) -> &str {
        "PreRestoreBackupHook"
    }

    fn priority(&self) -> u32 {
        20
    }

    async fn on_before_restore(&self, ctx: &BeforeRestoreCtx) -> Result<(), BackupError> {
        // V2 conflict resolution creates a mandatory, checked backup and keeps
        // its identifier for rollback. A second hook backup could evict that
        // exact archive under a one-item retention policy.
        if ctx.source == super::HookSource::CloudConflictResolution {
            return Ok(());
        }
        let Some(plan) = &ctx.capture_plan else {
            return Ok(());
        };
        info!(
            target: "rgsm::hooks::pre_restore_backup",
            "Creating extra backup before restoring {} / {}",
            ctx.game.name, ctx.snapshot.date
        );

        ctx.game
            .create_overwrite_snapshot_from_capture_plan(
                plan,
                &crate::config::resolve_backup_path(&ctx.config.backup_path),
                ctx.config.settings.compression_preset,
                ctx.config.settings.max_extra_backup_count,
                crate::device::get_current_device_id(),
            )
            .map(drop)
            .map_err(|error| BackupError::PreRestoreBackupFailed(Box::new(error)))
    }
}
