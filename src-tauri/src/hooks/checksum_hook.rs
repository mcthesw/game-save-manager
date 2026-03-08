use anyhow::Result as HookResult;
use async_trait::async_trait;
use log::{info, warn};

use super::pipeline::{BeforeRestoreCtx, SnapshotCreatedCtx, SnapshotHook};
use crate::backup::compute_file_hash;
use crate::config::get_config;
use crate::preclude::BackupError;

/// Computes and verifies XXH3 archive hashes.
///
/// - **on_snapshot_created**: verifies the stored hash matches the file on disk.
/// - **on_before_restore** (gate): verifies archive integrity before decompress.
///
/// Priority 10 — runs early so downstream hooks see verified data.
pub struct ChecksumHook;

#[async_trait]
impl SnapshotHook for ChecksumHook {
    fn name(&self) -> &str {
        "ChecksumHook"
    }

    fn priority(&self) -> u32 {
        10
    }

    async fn on_snapshot_created(&self, ctx: &SnapshotCreatedCtx) -> HookResult<()> {
        if let Some(stored) = &ctx.snapshot.archive_hash {
            match compute_file_hash(&ctx.local_zip_path) {
                Ok(actual) if &actual == stored => {
                    info!(
                        target: "rgsm::hooks::checksum",
                        "Archive hash verified for {}: {stored}",
                        ctx.game.name
                    );
                }
                Ok(actual) => {
                    warn!(
                        target: "rgsm::hooks::checksum",
                        "Archive hash MISMATCH for {}: stored={stored}, actual={actual}",
                        ctx.game.name
                    );
                }
                Err(e) => {
                    warn!(
                        target: "rgsm::hooks::checksum",
                        "Could not verify archive hash for {}: {e:#}",
                        ctx.game.name
                    );
                }
            }
        }
        Ok(())
    }

    async fn on_before_restore(&self, ctx: &BeforeRestoreCtx) -> Result<(), BackupError> {
        let verify = get_config()
            .map(|c| c.settings.verify_archive_before_apply)
            .unwrap_or(false);
        if !verify {
            return Ok(());
        }
        let Some(expected) = &ctx.snapshot.archive_hash else {
            info!(
                target: "rgsm::hooks::checksum",
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
            target: "rgsm::hooks::checksum",
            "Pre-restore integrity verified for {} / {}: {expected}",
            ctx.game.name, ctx.snapshot.date
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_dirs::resolve_app_path;
    use crate::backup::{Game, GameSnapshots, Snapshot};
    use crate::config::Config;
    use std::fs;
    use std::path::PathBuf;

    struct ConfigGuard {
        path: PathBuf,
        original_contents: Option<Vec<u8>>,
    }

    impl ConfigGuard {
        fn write_config(config: &Config) -> Result<Self, Box<dyn std::error::Error>> {
            let path = resolve_app_path("GameSaveManager.config.json");
            let original_contents = fs::read(&path).ok();
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, serde_json::to_vec_pretty(config)?)?;
            Ok(Self {
                path,
                original_contents,
            })
        }
    }

    impl Drop for ConfigGuard {
        fn drop(&mut self) {
            if let Some(contents) = &self.original_contents {
                let _ = fs::write(&self.path, contents);
            } else {
                let _ = fs::remove_file(&self.path);
            }
        }
    }

    #[tokio::test]
    async fn before_restore_returns_typed_integrity_error_when_hash_mismatches()
    -> Result<(), Box<dyn std::error::Error>> {
        let _config_lock = crate::config::lock_config_test_file_async().await;
        let mut config = Config::default();
        config.settings.verify_archive_before_apply = true;
        let _guard = ConfigGuard::write_config(&config)?;

        let temp_dir = temp_dir::TempDir::new()?;
        let archive_path = temp_dir.path().join("snapshot.zip");
        fs::write(&archive_path, b"corrupted-archive")?;
        let actual_hash = compute_file_hash(&archive_path)?;

        let ctx = BeforeRestoreCtx {
            source: super::super::pipeline::HookSource::UserManual,
            game: Game {
                name: "ChecksumGame".into(),
                save_paths: vec![],
                game_paths: Default::default(),
                next_save_unit_id: 0,
                cloud_sync_enabled: true,
            },
            snapshot: Snapshot {
                date: "2025-01-01T00:00:00".into(),
                describe: String::new(),
                path: archive_path.to_string_lossy().to_string(),
                size: fs::metadata(&archive_path)?.len(),
                parent: None,
                archive_hash: Some("expected-hash".into()),
                device_id: None,
            },
            snapshots: GameSnapshots {
                name: "ChecksumGame".into(),
                backups: vec![],
                head: None,
                sync_version: 0,
                last_sync_device: None,
                last_sync_timestamp: None,
            },
            archive_path: archive_path.clone(),
        };

        let err = ChecksumHook.on_before_restore(&ctx).await.unwrap_err();
        assert!(matches!(
            err,
            BackupError::IntegrityCheckFailed { expected, actual }
                if expected == "expected-hash" && actual == actual_hash
        ));
        Ok(())
    }
}
