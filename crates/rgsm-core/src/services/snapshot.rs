use crate::backup::{
    self, CapturePlan, CaptureSnapshotOptions, CreatedBy, Game, GameSnapshots, RestoreNotifier,
    SaveUnitCaptureInput,
};
use crate::config::{get_backup_path, get_config, resolve_backup_path};
use crate::hooks::{
    BeforeRestoreCtx, HookSource, MetadataChangedCtx, SnapshotAppliedCtx, SnapshotDeletedCtx,
};
use crate::preclude::BackupError;

use super::ServiceContext;

impl ServiceContext {
    pub async fn quick_backup(
        &self,
        game: &Game,
        describe: &str,
        created_by: CreatedBy,
        source: HookSource,
    ) -> Result<(), BackupError> {
        self.create_snapshot_at(game, describe, None, created_by, source)
            .await
    }

    pub async fn quick_apply(
        &self,
        game: &Game,
        source: HookSource,
        notifier: Option<&dyn RestoreNotifier>,
    ) -> Result<(), BackupError> {
        let snapshots = game.get_game_snapshots_info()?;
        let snapshot = snapshots
            .backups
            .last()
            .ok_or(BackupError::NoBackupAvailable)?
            .clone();
        self.restore_snapshot(game, &snapshot.date, source, notifier)
            .await
    }

    pub async fn create_snapshot(
        &self,
        game: &Game,
        describe: &str,
        source: HookSource,
    ) -> Result<(), BackupError> {
        self.create_snapshot_at(game, describe, None, CreatedBy::Manual, source)
            .await
    }

    pub async fn create_snapshot_at(
        &self,
        game: &Game,
        describe: &str,
        parent_date: Option<String>,
        created_by: CreatedBy,
        source: HookSource,
    ) -> Result<(), BackupError> {
        let config = get_config()?;
        let plan = CapturePlan::from_resolution_reports(
            game.save_paths
                .iter()
                .filter(|save_unit| save_unit.enabled)
                .map(|save_unit| SaveUnitCaptureInput {
                    save_unit_id: save_unit.id,
                    delete_before_apply: save_unit.delete_before_apply,
                    report: self.resolve_save_unit(&config, game, save_unit),
                })
                .collect(),
        )?;
        let created = game
            .create_snapshot_from_capture_plan(
                &plan,
                CaptureSnapshotOptions {
                    backup_base: &resolve_backup_path(&config.backup_path),
                    preset: config.settings.compression_preset,
                    describe,
                    parent_date,
                    created_by,
                    source_fingerprint: None,
                },
            )
            .await?;

        if let Some(snapshot) = created.snapshots.backups.last().cloned() {
            let mut ctx = crate::hooks::SnapshotCreatedCtx {
                config,
                source,
                game: game.clone(),
                snapshot,
                snapshots: created.snapshots,
                local_zip_path: created.local_zip_path,
                remote_zip_path: created.remote_zip_path,
            };
            self.pipeline().fire_snapshot_created(&mut ctx).await;
            game.set_game_snapshots_info(&ctx.snapshots)?;
        }

        Ok(())
    }

    pub async fn restore_snapshot(
        &self,
        game: &Game,
        date: &str,
        source: HookSource,
        notifier: Option<&dyn RestoreNotifier>,
    ) -> Result<(), BackupError> {
        let snapshots_before_restore = game.get_game_snapshots_info()?;
        let snapshot = snapshots_before_restore
            .backups
            .iter()
            .find(|snapshot| snapshot.date == date)
            .cloned()
            .ok_or_else(|| BackupError::BackupNotExist {
                name: game.name.clone(),
                date: date.to_string(),
            })?;

        let archive_path = get_backup_path()?
            .join(game.backup_dir_name().as_ref())
            .join(format!("{date}.zip"));
        let config = get_config()?;

        self.pipeline()
            .fire_before_restore(&BeforeRestoreCtx {
                config: config.clone(),
                source: source.clone(),
                game: game.clone(),
                snapshot: snapshot.clone(),
                snapshots: snapshots_before_restore,
                archive_path,
            })
            .await?;

        let snapshots = game.restore_snapshot(date, notifier)?;

        self.pipeline()
            .fire_snapshot_applied(&SnapshotAppliedCtx {
                config,
                source,
                game: game.clone(),
                snapshot,
                snapshots,
            })
            .await;

        Ok(())
    }

    pub async fn backup_all(&self, source: HookSource) -> Result<(), BackupError> {
        let created_snapshots = backup::backup_all().await?;
        let config = get_config()?;

        for created in created_snapshots {
            let game = config
                .games
                .iter()
                .find(|game| game.backup_dir_name() == created.snapshots.name)
                .cloned()
                .ok_or_else(|| {
                    BackupError::Unexpected(anyhow::anyhow!(
                        "Game '{}' was not found while finalizing backup_all",
                        created.snapshots.name
                    ))
                })?;

            if let Some(snapshot) = created.snapshots.backups.last().cloned() {
                let mut ctx = crate::hooks::SnapshotCreatedCtx {
                    config: config.clone(),
                    source: source.clone(),
                    game: game.clone(),
                    snapshot,
                    snapshots: created.snapshots,
                    local_zip_path: created.local_zip_path,
                    remote_zip_path: created.remote_zip_path,
                };
                self.pipeline().fire_snapshot_created(&mut ctx).await;
                game.set_game_snapshots_info(&ctx.snapshots)?;
            }
        }

        Ok(())
    }

    pub async fn apply_all(
        &self,
        source: HookSource,
        notifier: Option<&dyn RestoreNotifier>,
    ) -> Result<(), BackupError> {
        let config = get_config()?;
        let backup_base = get_backup_path()?;

        for game in &config.games {
            let snapshots_info = game.get_game_snapshots_info()?;
            let Some(snapshot) = snapshots_info.backups.last().cloned() else {
                continue;
            };
            let archive_path = backup_base
                .join(game.backup_dir_name().as_ref())
                .join(format!("{}.zip", snapshot.date));

            self.pipeline()
                .fire_before_restore(&BeforeRestoreCtx {
                    config: config.clone(),
                    source: source.clone(),
                    game: game.clone(),
                    snapshot: snapshot.clone(),
                    snapshots: snapshots_info,
                    archive_path,
                })
                .await?;

            let snapshots = game.restore_snapshot(&snapshot.date, notifier)?;
            self.pipeline()
                .fire_snapshot_applied(&SnapshotAppliedCtx {
                    config: config.clone(),
                    source: source.clone(),
                    game: game.clone(),
                    snapshot,
                    snapshots,
                })
                .await;
        }

        Ok(())
    }

    pub async fn delete_snapshot(
        &self,
        game: &Game,
        date: &str,
        source: HookSource,
    ) -> Result<(), BackupError> {
        let deleted = game.delete_snapshot(date).await?;
        let config = get_config()?;

        self.pipeline()
            .fire_snapshot_deleted(&SnapshotDeletedCtx {
                config,
                source,
                game: game.clone(),
                snapshots: deleted.snapshots,
                deleted_remote_paths: vec![deleted.remote_zip_path],
            })
            .await;

        Ok(())
    }

    pub async fn batch_delete_snapshots(
        &self,
        game: &Game,
        dates: &[String],
        source: HookSource,
    ) -> Result<(), BackupError> {
        let deleted = game.batch_delete_snapshots(dates).await?;
        let config = get_config()?;

        self.pipeline()
            .fire_snapshot_deleted(&SnapshotDeletedCtx {
                config,
                source,
                game: game.clone(),
                snapshots: deleted.snapshots,
                deleted_remote_paths: deleted.deleted_remote_paths,
            })
            .await;

        Ok(())
    }

    pub async fn set_snapshot_description(
        &self,
        game: &Game,
        date: &str,
        describe: &str,
        source: HookSource,
    ) -> Result<(), BackupError> {
        let snapshots = game.set_snapshot_description(date, describe).await?;
        let config = get_config()?;

        self.pipeline()
            .fire_metadata_changed(&MetadataChangedCtx {
                config,
                source,
                game: game.clone(),
                snapshots,
            })
            .await;

        Ok(())
    }

    pub async fn set_snapshot_created_by(
        &self,
        game_name: &str,
        snapshot_date: &str,
        created_by: CreatedBy,
        source: HookSource,
    ) -> Result<GameSnapshots, BackupError> {
        let config = get_config()?;
        let game = config
            .games
            .iter()
            .find(|game| game.name == game_name)
            .cloned()
            .ok_or_else(|| BackupError::BackupNotExist {
                name: game_name.to_string(),
                date: snapshot_date.to_string(),
            })?;

        let mut snapshots = game.get_game_snapshots_info()?;
        let snapshot = snapshots
            .backups
            .iter_mut()
            .find(|snapshot| snapshot.date == snapshot_date)
            .ok_or_else(|| BackupError::BackupNotExist {
                name: game_name.to_string(),
                date: snapshot_date.to_string(),
            })?;
        snapshot.created_by = created_by;

        game.set_game_snapshots_info(&snapshots)?;

        self.pipeline()
            .fire_metadata_changed(&MetadataChangedCtx {
                config,
                source,
                game: game.clone(),
                snapshots: snapshots.clone(),
            })
            .await;

        Ok(snapshots)
    }

    pub async fn set_snapshot_head(
        &self,
        game: &Game,
        date: &str,
        source: HookSource,
    ) -> Result<(), BackupError> {
        let mut snapshots = game.get_game_snapshots_info()?;
        if !snapshots
            .backups
            .iter()
            .any(|snapshot| snapshot.date == date)
        {
            return Err(BackupError::BackupNotExist {
                name: game.name.clone(),
                date: date.to_string(),
            });
        }

        snapshots.set_current_device_head(Some(date.to_string()));
        game.set_game_snapshots_info(&snapshots)?;

        let config = get_config()?;
        self.pipeline()
            .fire_metadata_changed(&MetadataChangedCtx {
                config,
                source,
                game: game.clone(),
                snapshots,
            })
            .await;

        Ok(())
    }

    pub async fn detach_snapshot(
        &self,
        game: &Game,
        date: &str,
        source: HookSource,
    ) -> Result<(), BackupError> {
        let mut snapshots = game.get_game_snapshots_info()?;
        let snapshot = snapshots
            .backups
            .iter_mut()
            .find(|snapshot| snapshot.date == date)
            .ok_or_else(|| BackupError::BackupNotExist {
                name: game.name.clone(),
                date: date.to_string(),
            })?;
        snapshot.parent = None;

        game.set_game_snapshots_info(&snapshots)?;

        let config = get_config()?;
        self.pipeline()
            .fire_metadata_changed(&MetadataChangedCtx {
                config,
                source,
                game: game.clone(),
                snapshots,
            })
            .await;

        Ok(())
    }
}
