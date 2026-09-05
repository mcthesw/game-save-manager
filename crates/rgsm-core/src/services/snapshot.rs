use crate::backup::{
    ArchiveBackend, ArchiveCaptureGroup, ArchiveFormat, ArchiveVersion, CaptureSnapshotOptions,
    CaptureSourceKind, CreatedBy, Game, GameSnapshots, RestoreNotificationLevel, RestoreNotifier,
    RestorePlan, SaveUnit, SaveUnitType, SevenZBackend, TimerSnapshotDecision, ZipBackend,
    archive_file_name, snapshot_archive_path,
};
use crate::config::{get_backup_path, get_config, resolve_backup_path};
use crate::hooks::{
    BeforeRestoreCtx, HookSource, MetadataChangedCtx, SnapshotAppliedCtx, SnapshotDeletedCtx,
};
use crate::preclude::BackupError;

use super::ServiceContext;

/// Emit a stage notification when a notifier is attached. Stage payloads are
/// pure progress text; the frontend folds them into the running activity entry.
fn notify_stage(notifier: Option<&dyn RestoreNotifier>, msg: &str) {
    if let Some(notifier) = notifier {
        notifier.notify(
            RestoreNotificationLevel::Info,
            rust_i18n::t!("backend.stage.title").as_ref(),
            msg,
        );
    }
}

impl ServiceContext {
    pub async fn quick_backup(
        &self,
        game: &Game,
        describe: &str,
        created_by: CreatedBy,
        source: HookSource,
    ) -> Result<(), BackupError> {
        self.create_snapshot_at(game, describe, None, created_by, source, None)
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
            .latest_snapshot()
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
        notifier: Option<&dyn RestoreNotifier>,
    ) -> Result<(), BackupError> {
        self.create_snapshot_at(game, describe, None, CreatedBy::Manual, source, notifier)
            .await
    }

    pub async fn create_snapshot_at(
        &self,
        game: &Game,
        describe: &str,
        parent_date: Option<String>,
        created_by: CreatedBy,
        source: HookSource,
        notifier: Option<&dyn RestoreNotifier>,
    ) -> Result<(), BackupError> {
        notify_stage(notifier, rust_i18n::t!("backend.stage.scan").as_ref());
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
                    notifier,
                },
            )
            .await?;

        if let Some(snapshot) = created.snapshots.backups.last().cloned() {
            notify_stage(notifier, rust_i18n::t!("backend.stage.finalize").as_ref());
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
        notifier: Option<&dyn RestoreNotifier>,
    ) -> Result<TimerSnapshotDecision, BackupError> {
        notify_stage(notifier, rust_i18n::t!("backend.stage.scan").as_ref());
        let config = get_config()?;
        let plan = self.capture_plan(&config, game)?;
        let fingerprint = crate::backup::state_fingerprint::fingerprint_capture_plan(&plan)?;
        let snapshots = game.get_game_snapshots_info()?;
        let latest = snapshots
            .backups
            .iter()
            .filter(|snapshot| snapshot.created_by.is_automatic_backup())
            .max_by_key(|snapshot| snapshot.creation_time());
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
                    notifier,
                },
            )
            .await?;
        if let Some(snapshot) = created.snapshots.backups.last().cloned() {
            notify_stage(notifier, rust_i18n::t!("backend.stage.finalize").as_ref());
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
            self.restore_capture_archive(&config, game, &archive_path, &SevenZBackend, notifier)?;
            let mut snapshots = game.get_game_snapshots_info()?;
            snapshots.set_current_device_head(Some(date.to_string()));
            game.set_game_snapshots_info(&snapshots)?;
            snapshots
        } else {
            match ZipBackend.archive_version(&archive_path)? {
                ArchiveVersion::V2 | ArchiveVersion::V3 => {
                    self.restore_capture_archive(
                        &config,
                        game,
                        &archive_path,
                        &ZipBackend,
                        notifier,
                    )?;
                    let mut snapshots = game.get_game_snapshots_info()?;
                    snapshots.set_current_device_head(Some(date.to_string()));
                    game.set_game_snapshots_info(&snapshots)?;
                    snapshots
                }
                ArchiveVersion::Legacy | ArchiveVersion::V1 => {
                    let device = config.devices.get(crate::device::get_current_device_id());
                    let path_context = game.path_context(device);
                    game.restore_snapshot_with_context(
                        date,
                        notifier,
                        &resolve_backup_path(&config.backup_path),
                        &path_context,
                    )?
                }
                ArchiveVersion::V4 => unreachable!("Archive V4 uses the 7z backend"),
            }
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
            self.restore_capture_archive(&config, game, &archive_path, &SevenZBackend, notifier)
        } else {
            match ZipBackend.archive_version(&archive_path)? {
                ArchiveVersion::V2 | ArchiveVersion::V3 => self.restore_capture_archive(
                    &config,
                    game,
                    &archive_path,
                    &ZipBackend,
                    notifier,
                ),
                ArchiveVersion::Legacy | ArchiveVersion::V1 => {
                    let device = config.devices.get(crate::device::get_current_device_id());
                    ZipBackend.decompress(
                        &game.save_paths,
                        &archive_path,
                        notifier,
                        Some(&game.path_context(device)),
                    )?;
                    Ok(())
                }
                ArchiveVersion::V4 => unreachable!("Archive V4 uses the 7z backend"),
            }
        }
    }

    fn restore_capture_archive(
        &self,
        config: &crate::config::Config,
        game: &Game,
        archive_path: &std::path::Path,
        backend: &dyn ArchiveBackend,
        notifier: Option<&dyn RestoreNotifier>,
    ) -> Result<(), BackupError> {
        let mut manifest = backend.read_capture_manifest(archive_path)?;
        if manifest.version == 2 {
            apply_legacy_v2_save_unit_metadata(&mut manifest.groups, &game.save_paths);
        }
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
        let plan = if manifest.version == 2 {
            RestorePlan::build_legacy_v2(&manifest.groups, &reports, rules)?
        } else {
            RestorePlan::build(&manifest.groups, &reports, rules)?
        };
        if let Some(notifier) = notifier {
            for save_unit_id in &plan.skipped_inactive_save_unit_ids {
                notifier.notify(
                    RestoreNotificationLevel::Warning,
                    rust_i18n::t!("backend.archive.restore_skipped_title").as_ref(),
                    rust_i18n::t!(
                        "backend.archive.restore_unit_skipped",
                        unit = save_unit_id,
                        reason = rust_i18n::t!("backend.archive.skip_reason_inactive_save_unit")
                    )
                    .as_ref(),
                );
            }
        }
        backend.restore_capture_plan(&plan, archive_path)?;
        Ok(())
    }

    pub async fn backup_all(&self, source: HookSource) -> Result<(), BackupError> {
        let config = get_config()?;
        let mut first_error = None;
        for game in &config.games {
            let result = self
                .create_snapshot_at(
                    game,
                    "Backup all",
                    None,
                    CreatedBy::Manual,
                    source.clone(),
                    None,
                )
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
            let Some(snapshot) = snapshots_info.latest_snapshot().cloned() else {
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

        // Resolve the name-only legacy endpoint once before checking ownership.
        // An unrelated local ID may be identical to this Game's display name.
        if self
            .is_shared_game(&game.storage_key)
            .map_err(|error| BackupError::Unexpected(error.into()))?
        {
            return Err(BackupError::SharedSnapshotProvenance);
        }

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

/// Archive V2 stores stable save-unit IDs but no type metadata. Its entry
/// shape is only a fallback for dynamic patterns; concrete save units retain
/// their declared type as the restore authority.
fn apply_legacy_v2_save_unit_metadata(groups: &mut [ArchiveCaptureGroup], units: &[SaveUnit]) {
    for group in groups {
        let Some(unit) = units.iter().find(|unit| unit.id == group.save_unit_id) else {
            continue;
        };
        group.delete_before_apply = unit.delete_before_apply;
        group.kind = match unit.unit_type() {
            Some(SaveUnitType::File) => CaptureSourceKind::File,
            Some(SaveUnitType::Folder) => CaptureSourceKind::Directory,
            Some(SaveUnitType::WinRegistry) => CaptureSourceKind::Registry,
            None => group.kind,
        };
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::backup::SaveUnitSource;
    use crate::path_pattern::{ManifestPathConstraints, ManifestPathPattern};
    use crate::path_resolution::CandidateDimensions;

    fn legacy_v2_group(kind: CaptureSourceKind) -> ArchiveCaptureGroup {
        ArchiveCaptureGroup {
            id: 0,
            save_unit_id: 12,
            candidate_id: "legacy-v2".to_string(),
            dimensions: CandidateDimensions::default(),
            relative_path: String::new(),
            archive_path: "12/registry.reg".to_string(),
            kind,
            delete_before_apply: false,
            source_path_diagnostic: None,
        }
    }

    #[test]
    fn batch_error_state_retains_the_first_failure() {
        let mut first_error = None;
        retain_first_error(&mut first_error, Err(BackupError::NoDataMatched));
        retain_first_error(&mut first_error, Ok(()));
        retain_first_error(&mut first_error, Err(BackupError::NoBackupAvailable));

        assert!(matches!(first_error, Some(BackupError::NoDataMatched)));
    }

    #[test]
    fn legacy_v2_concrete_type_overrides_archive_filename_inference() {
        let mut groups = vec![legacy_v2_group(CaptureSourceKind::Registry)];
        let units = vec![SaveUnit::concrete(
            12,
            SaveUnitType::File,
            HashMap::new(),
            true,
            true,
        )];

        apply_legacy_v2_save_unit_metadata(&mut groups, &units);

        assert_eq!(groups[0].kind, CaptureSourceKind::File);
        assert!(groups[0].delete_before_apply);
    }

    #[test]
    fn legacy_v2_dynamic_pattern_keeps_archive_shape_inference() {
        let mut groups = vec![legacy_v2_group(CaptureSourceKind::Directory)];
        let units = vec![SaveUnit {
            id: 12,
            source: SaveUnitSource::ManifestPattern {
                expected_type: None,
                pattern: ManifestPathPattern::new("<home>/Saves/*"),
                constraints: ManifestPathConstraints::default(),
            },
            delete_before_apply: true,
            enabled: true,
        }];

        apply_legacy_v2_save_unit_metadata(&mut groups, &units);

        assert_eq!(groups[0].kind, CaptureSourceKind::Directory);
        assert!(groups[0].delete_before_apply);
    }

    #[test]
    fn legacy_v2_typed_pattern_preserves_declared_folder_kind() {
        let mut groups = vec![legacy_v2_group(CaptureSourceKind::File)];
        let units = vec![SaveUnit {
            id: 12,
            source: SaveUnitSource::ManifestPattern {
                expected_type: Some(SaveUnitType::Folder),
                pattern: ManifestPathPattern::new("<root>/Saved"),
                constraints: ManifestPathConstraints::default(),
            },
            delete_before_apply: true,
            enabled: true,
        }];

        apply_legacy_v2_save_unit_metadata(&mut groups, &units);

        assert_eq!(groups[0].kind, CaptureSourceKind::Directory);
        assert!(groups[0].delete_before_apply);
    }
}
