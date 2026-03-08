use async_trait::async_trait;
use log::{info, warn};

use super::pipeline::{BeforeRestoreCtx, SnapshotHook};
use crate::preclude::BackupError;

/// Creates an extra (overwrite) backup before a snapshot is restored.
///
/// Priority 5 — runs before integrity check so the safety net exists
/// even if the checksum verification later aborts the restore.
///
/// This hook handles its own errors internally (logs a warning) and
/// always returns `Ok(())` — a failed extra backup should not block
/// the restore operation.
pub struct PreRestoreBackupHook;

#[async_trait]
impl SnapshotHook for PreRestoreBackupHook {
    fn name(&self) -> &str {
        "PreRestoreBackupHook"
    }

    fn priority(&self) -> u32 {
        5
    }

    async fn on_before_restore(&self, ctx: &BeforeRestoreCtx) -> Result<(), BackupError> {
        info!(
            target: "rgsm::hooks::pre_restore_backup",
            "Creating extra backup before restoring {} / {}",
            ctx.game.name, ctx.snapshot.date
        );

        if let Err(e) = ctx
            .game
            .create_overwrite_snapshot(ctx.config.settings.max_extra_backup_count)
        {
            warn!(
                target: "rgsm::hooks::pre_restore_backup",
                "Failed to create extra backup for {}: {e:#}",
                ctx.game.name
            );
        }

        Ok(())
    }
}
