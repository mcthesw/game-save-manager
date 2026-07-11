//! Built-in hooks for archive hash generation and verification.
//!
//! These hooks run early so later side effects, such as uploads, can reuse the
//! finalized integrity metadata.

use anyhow::Result as HookResult;
use async_trait::async_trait;
use log::info;

use super::pipeline::{BeforeRestoreCtx, LifecycleHook, SnapshotCreatedCtx};
use crate::backup::compute_file_hash;
use crate::preclude::BackupError;

/// Computes an archive hash after a snapshot archive has been created.
pub struct ArchiveHashHook;

#[async_trait]
impl LifecycleHook for ArchiveHashHook {
    fn name(&self) -> &str {
        "ArchiveHashHook"
    }

    fn priority(&self) -> u32 {
        10
    }

    async fn on_snapshot_created(&self, ctx: &mut SnapshotCreatedCtx) -> HookResult<()> {
        let hash = compute_file_hash(&ctx.local_zip_path)?;
        ctx.snapshot.archive_hash = Some(hash.clone());
        if let Some(snapshot) = ctx
            .snapshots
            .backups
            .iter_mut()
            .find(|snapshot| snapshot.date == ctx.snapshot.date)
        {
            snapshot.archive_hash = Some(hash.clone());
        }
        info!(
            target: "rgsm::hooks::archive_hash",
            "Computed archive hash for {} / {}: {hash}",
            ctx.game.name, ctx.snapshot.date
        );
        Ok(())
    }
}

/// Verifies archive integrity before a snapshot is restored.
pub struct ArchiveVerifyHook;

#[async_trait]
impl LifecycleHook for ArchiveVerifyHook {
    fn name(&self) -> &str {
        "ArchiveVerifyHook"
    }

    fn priority(&self) -> u32 {
        15
    }

    async fn on_before_restore(&self, ctx: &BeforeRestoreCtx) -> Result<(), BackupError> {
        let Some(expected) = &ctx.snapshot.archive_hash else {
            info!(
                target: "rgsm::hooks::archive_verify",
                "No stored hash for {} / {} — skipping pre-restore check",
                ctx.game.name, ctx.snapshot.date
            );
            return Ok(());
        };

        let actual = compute_file_hash(&ctx.archive_path).map_err(BackupError::Compress)?;
        if &actual != expected {
            return Err(BackupError::IntegrityCheckFailed {
                expected: expected.clone(),
                actual,
            });
        }

        info!(
            target: "rgsm::hooks::archive_verify",
            "Pre-restore integrity verified for {} / {}: {expected}",
            ctx.game.name, ctx.snapshot.date
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{Game, GameSnapshots, Snapshot};
    use crate::config::Config;
    use crate::hooks::HookSource;
    use std::fs;

    fn test_game() -> Game {
        Game {
            name: "ChecksumGame".into(),
            storage_key: String::new(),
            save_paths: vec![],
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        }
    }

    #[tokio::test]
    async fn snapshot_created_populates_archive_hash_when_enabled()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = temp_dir::TempDir::new()?;
        let archive_path = temp_dir.path().join("snapshot.zip");
        fs::write(&archive_path, b"snapshot-contents")?;
        let expected_hash = compute_file_hash(&archive_path)?;

        let snapshot = Snapshot {
            date: "2025-01-01T00:00:00".into(),
            describe: String::new(),
            path: archive_path.to_string_lossy().to_string(),
            size: fs::metadata(&archive_path)?.len(),
            parent: None,
            archive_hash: None,
            device_id: None,
            created_by: Default::default(),
        };
        let mut snapshots = GameSnapshots::new("ChecksumGame");
        snapshots.backups.push(snapshot.clone());
        let mut ctx = SnapshotCreatedCtx {
            config: Config::default(),
            source: HookSource::UserManual,
            game: test_game(),
            snapshot: snapshot.clone(),
            snapshots,
            local_zip_path: archive_path.clone(),
            remote_zip_path: "save_data/ChecksumGame/2025-01-01T00:00:00.zip".into(),
        };

        ArchiveHashHook.on_snapshot_created(&mut ctx).await?;

        assert_eq!(
            ctx.snapshot.archive_hash.as_deref(),
            Some(expected_hash.as_str())
        );
        assert_eq!(
            ctx.snapshots.backups[0].archive_hash.as_deref(),
            Some(expected_hash.as_str())
        );
        Ok(())
    }

    #[tokio::test]
    async fn before_restore_returns_typed_integrity_error_when_hash_mismatches()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = temp_dir::TempDir::new()?;
        let archive_path = temp_dir.path().join("snapshot.zip");
        fs::write(&archive_path, b"corrupted-archive")?;
        let actual_hash = compute_file_hash(&archive_path)?;

        let ctx = BeforeRestoreCtx {
            capture_plan: None,
            config: Config::default(),
            source: HookSource::UserManual,
            game: test_game(),
            snapshot: Snapshot {
                date: "2025-01-01T00:00:00".into(),
                describe: String::new(),
                path: archive_path.to_string_lossy().to_string(),
                size: fs::metadata(&archive_path)?.len(),
                parent: None,
                archive_hash: Some("expected-hash".into()),
                device_id: None,
                created_by: Default::default(),
            },
            snapshots: GameSnapshots::new("ChecksumGame"),
            archive_path: archive_path.clone(),
        };

        let err = ArchiveVerifyHook.on_before_restore(&ctx).await.unwrap_err();
        assert!(matches!(
            err,
            BackupError::IntegrityCheckFailed { expected, actual }
                if expected == "expected-hash" && actual == actual_hash
        ));
        Ok(())
    }
}
