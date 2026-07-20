use crate::backup::{
    ArchiveBackend, ArchiveFormat, ArchiveVersion, CaptureSnapshotOptions, CreatedBy, Game,
    GameSnapshots, RestoreNotifier, RestorePlan, SevenZBackend, TimerSnapshotDecision, ZipBackend,
    archive_file_name, snapshot_archive_path,
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
        let plan = self.capture_plan(&config, game)?;
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
            game.set_game_snapshots_info(&created.snapshots)?;
            let mut ctx = crate::hooks::SnapshotCreatedCtx {
                config,
                source,
                game: game.clone(),
                snapshot,
                snapshots: created.snapshots,
                local_archive_path: created.local_archive_path,
                remote_archive_path: created.remote_archive_path,
            };
            self.pipeline().fire_snapshot_created(&mut ctx).await;
            game.set_game_snapshots_info(&ctx.snapshots)?;
        }

        Ok(())
    }

    pub async fn create_snapshot_if_changed(
        &self,
        game: &Game,
        describe: &str,
        created_by: CreatedBy,
        source: HookSource,
    ) -> Result<TimerSnapshotDecision, BackupError> {
        let config = get_config()?;
        let plan = self.capture_plan(&config, game)?;
        let fingerprint = crate::backup::state_fingerprint::fingerprint_capture_plan(&plan)?;
        let snapshots = game.get_game_snapshots_info()?;
        let latest = snapshots
            .backups
            .iter()
            .filter(|snapshot| snapshot.created_by.is_automatic_backup())
            .max_by_key(|snapshot| &snapshot.date);
        if latest.is_some_and(|snapshot| {
            let backend: &dyn ArchiveBackend = match snapshot.archive_format {
                ArchiveFormat::Zip => &ZipBackend,
                ArchiveFormat::SevenZ => &SevenZBackend,
            };
            backend
                .read_source_fingerprint(std::path::Path::new(&snapshot.path))
                .as_deref()
                == Some(fingerprint.as_str())
        }) {
            return Ok(TimerSnapshotDecision::SkippedUnchanged);
        }

        let created = game
            .create_snapshot_from_capture_plan(
                &plan,
                CaptureSnapshotOptions {
                    backup_base: &resolve_backup_path(&config.backup_path),
                    preset: config.settings.compression_preset,
                    describe,
                    parent_date: None,
                    created_by,
                    source_fingerprint: Some(fingerprint),
                },
            )
            .await?;
        if let Some(snapshot) = created.snapshots.backups.last().cloned() {
            game.set_game_snapshots_info(&created.snapshots)?;
            let mut ctx = crate::hooks::SnapshotCreatedCtx {
                config,
                source,
                game: game.clone(),
                snapshot,
                snapshots: created.snapshots,
                local_archive_path: created.local_archive_path,
                remote_archive_path: created.remote_archive_path,
            };
            self.pipeline().fire_snapshot_created(&mut ctx).await;
            game.set_game_snapshots_info(&ctx.snapshots)?;
        }
        Ok(TimerSnapshotDecision::Created)
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

        let game_dir = get_backup_path()?.join(game.backup_dir_name().as_ref());
        let archive_path = snapshot_archive_path(&game_dir, &snapshot);
        let config = get_config()?;

        self.pipeline()
            .fire_before_restore(&BeforeRestoreCtx {
                config: config.clone(),
                source: source.clone(),
                game: game.clone(),
                snapshot: snapshot.clone(),
                snapshots: snapshots_before_restore,
                archive_path: archive_path.clone(),
                capture_plan: self.capture_plan(&config, game).ok(),
            })
            .await?;

        let snapshots = if snapshot.archive_format == ArchiveFormat::SevenZ {
            self.restore_capture_archive(&config, game, &archive_path, &SevenZBackend)?;
            let mut snapshots = game.get_game_snapshots_info()?;
            snapshots.set_current_device_head(Some(date.to_string()));
            game.set_game_snapshots_info(&snapshots)?;
            snapshots
        } else if ZipBackend.archive_version(&archive_path)? == ArchiveVersion::V3 {
            self.restore_capture_archive(&config, game, &archive_path, &ZipBackend)?;
            let mut snapshots = game.get_game_snapshots_info()?;
            snapshots.set_current_device_head(Some(date.to_string()));
            game.set_game_snapshots_info(&snapshots)?;
            snapshots
        } else {
            let device = config.devices.get(crate::device::get_current_device_id());
            let path_context = game.path_context(device);
            game.restore_snapshot_with_context(
                date,
                notifier,
                &resolve_backup_path(&config.backup_path),
                &path_context,
            )?
        };

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

    pub fn restore_extra_backup(
        &self,
        game: &Game,
        date: &str,
        notifier: Option<&dyn RestoreNotifier>,
    ) -> Result<(), BackupError> {
        let config = get_config()?;
        let folder = crate::backup::extra_backup_folder_path(game)?;
        let seven_z = folder.join(archive_file_name(date, ArchiveFormat::SevenZ));
        let archive_path = if seven_z.exists() {
            seven_z
        } else {
            folder.join(archive_file_name(date, ArchiveFormat::Zip))
        };
        if archive_path.extension().and_then(|value| value.to_str()) == Some("7z") {
            self.restore_capture_archive(&config, game, &archive_path, &SevenZBackend)
        } else if ZipBackend.archive_version(&archive_path)? == ArchiveVersion::V3 {
            self.restore_capture_archive(&config, game, &archive_path, &ZipBackend)
        } else {
            let device = config.devices.get(crate::device::get_current_device_id());
            ZipBackend.decompress(
                &game.save_paths,
                &archive_path,
                notifier,
                Some(&game.path_context(device)),
            )?;
            Ok(())
        }
    }

    fn restore_capture_archive(
        &self,
        config: &crate::config::Config,
        game: &Game,
        archive_path: &std::path::Path,
        backend: &dyn ArchiveBackend,
    ) -> Result<(), BackupError> {
        let manifest = backend.read_capture_manifest(archive_path)?;
        let reports = game
            .save_paths
            .iter()
            .filter(|unit| unit.enabled)
            .map(|unit| {
                (
                    unit.id,
                    self.resolve_save_unit_for_restore(config, game, unit),
                )
            })
            .collect();
        let rules = game
            .device_bindings
            .get(crate::device::get_current_device_id())
            .map(|binding| binding.restore_mappings.as_slice())
            .unwrap_or_default();
        let plan = RestorePlan::build(&manifest.groups, &reports, rules)?;
        backend.restore_capture_plan(&plan, archive_path)?;
        Ok(())
    }

    pub async fn backup_all(&self, source: HookSource) -> Result<(), BackupError> {
        let config = get_config()?;
        let mut first_error = None;
        for game in &config.games {
            let result = self
                .create_snapshot_at(game, "Backup all", None, CreatedBy::Manual, source.clone())
                .await;
            retain_first_error(&mut first_error, result);
        }
        first_error.map_or(Ok(()), Err)
    }

    pub async fn apply_all(
        &self,
        source: HookSource,
        notifier: Option<&dyn RestoreNotifier>,
    ) -> Result<(), BackupError> {
        let config = get_config()?;
        for game in &config.games {
            let snapshots_info = game.get_game_snapshots_info()?;
            let Some(snapshot) = snapshots_info.backups.last().cloned() else {
                continue;
            };
            self.restore_snapshot(game, &snapshot.date, source.clone(), notifier)
                .await?;
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
                deleted_remote_paths: vec![deleted.remote_archive_path],
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

fn retain_first_error(first_error: &mut Option<BackupError>, result: Result<(), BackupError>) {
    if let Err(error) = result {
        first_error.get_or_insert(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_error_state_retains_the_first_failure() {
        let mut first_error = None;
        retain_first_error(&mut first_error, Err(BackupError::NoDataMatched));
        retain_first_error(&mut first_error, Ok(()));
        retain_first_error(&mut first_error, Err(BackupError::NoBackupAvailable));

        assert!(matches!(first_error, Some(BackupError::NoDataMatched)));
    }
}
