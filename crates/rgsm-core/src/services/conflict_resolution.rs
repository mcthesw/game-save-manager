use std::path::PathBuf;

use serde::Serialize;
use specta::Type;

use crate::app_dirs::resolve_app_path;
use crate::backup::{CapturePlanError, Game, GameSnapshots, Snapshot, snapshot_archive_path};
use crate::cloud_sync::v2::{
    ArchiveIntegrity, KeepLocalProgressOutcome, V2ConflictResolver, V2RemoteProgressResolver,
};
use crate::config::{
    CloudNamespaceGeneration, cloud_bootstrap_inputs, get_config, resolve_backup_path,
};
use crate::hooks::HookSource;

use super::{CloudLibraryServiceError, ServiceContext, cloud_library_target::bound_v2_operator};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type, utoipa::ToSchema)]
pub struct AcceptRemoteProgressOutcome {
    pub snapshot_id: String,
    pub safety_backup_created: bool,
    pub manifest_revision: u64,
}

impl ServiceContext {
    pub async fn keep_v2_local_progress(
        &self,
        game_id: &str,
        manifest_revision: u64,
        local_snapshot_id: &str,
    ) -> Result<KeepLocalProgressOutcome, CloudLibraryServiceError> {
        self.require_shared_game(game_id)?;
        let (_, profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let local = get_config()?
            .games
            .into_iter()
            .find(|game| game.storage_key == game_id)
            .ok_or_else(|| CloudLibraryServiceError::GameProfileNotFound(game_id.to_string()))?
            .get_game_snapshots_info()?;
        let local_archive_root = profile
            .local_archive_root
            .as_deref()
            .map(resolve_app_path)
            .ok_or(CloudLibraryServiceError::StorageLocationRequired)?;
        let outcome = V2ConflictResolver::new(
            bound_v2_operator(&local_state).await?,
            local_archive_root,
            local_state.current_device_id,
            resolve_app_path("GameSaveManager.cloud-v2-materialization.json"),
            3,
        )
        .keep_local(game_id, manifest_revision, local_snapshot_id, &local)
        .await?;
        Ok(outcome)
    }

    pub async fn accept_v2_remote_progress(
        &self,
        game_id: &str,
        manifest_revision: u64,
        expected_local_snapshot_id: Option<&str>,
        selected_snapshot_id: &str,
    ) -> Result<AcceptRemoteProgressOutcome, CloudLibraryServiceError> {
        self.require_shared_game(game_id)?;
        let (_, profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let config = get_config()?;
        let game = config
            .games
            .iter()
            .find(|game| game.storage_key == game_id)
            .cloned()
            .ok_or_else(|| CloudLibraryServiceError::GameProfileNotFound(game_id.to_string()))?;
        let original = game.get_game_snapshots_info()?;
        let local_archive_root = profile
            .local_archive_root
            .as_deref()
            .map(resolve_app_path)
            .ok_or(CloudLibraryServiceError::StorageLocationRequired)?;
        let resolver = V2RemoteProgressResolver::new(
            bound_v2_operator(&local_state).await?,
            local_archive_root,
            local_state.current_device_id,
            resolve_app_path("GameSaveManager.cloud-v2-materialization.json"),
            3,
        );
        let prepared = resolver
            .prepare(
                game_id,
                manifest_revision,
                expected_local_snapshot_id,
                selected_snapshot_id,
                &original,
            )
            .await?;

        let backup_date = match self.capture_plan(&config, &game) {
            Ok(plan) => Some(game.create_overwrite_snapshot_from_capture_plan(
                &plan,
                &resolve_backup_path(&config.backup_path),
                config.settings.compression_preset,
                config.settings.max_extra_backup_count,
                crate::device::get_current_device_id(),
            )?),
            Err(CapturePlanError::NoDataMatched) => None,
            Err(error) => return Err(crate::preclude::BackupError::from(error).into()),
        };

        // The comparison and the merge share one catalog lock, so a snapshot
        // committed since `prepare` cannot be overwritten.
        let mut superseded = Vec::new();
        game.update_game_snapshots_info::<CloudLibraryServiceError>(|current| {
            if *current != original {
                return Err(CloudLibraryServiceError::LocalSnapshotHistoryChanged);
            }
            superseded = merge_remote_lineage(current, &prepared.lineage)?;
            Ok(())
        })?;
        if let Err(error) = self
            .restore_snapshot(
                &game,
                &prepared.selected_snapshot_id,
                HookSource::CloudConflictResolution,
                None,
            )
            .await
        {
            return Err(CloudLibraryServiceError::RemoteProgressApply {
                stage: "Apply",
                operation: error.to_string(),
                rollback: rollback_remote_progress(
                    self,
                    &game,
                    &original,
                    &prepared.lineage,
                    backup_date.as_deref(),
                )
                .await,
            });
        }

        let manifest_revision = match resolver
            .commit_current_device_head(game_id, &prepared.selected_snapshot_id)
            .await
        {
            Ok(revision) => revision,
            Err(error) => {
                return Err(CloudLibraryServiceError::RemoteProgressApply {
                    stage: "Head publication",
                    operation: error.to_string(),
                    rollback: rollback_remote_progress(
                        self,
                        &game,
                        &original,
                        &prepared.lineage,
                        backup_date.as_deref(),
                    )
                    .await,
                });
            }
        };
        remove_superseded_archives(&superseded);
        Ok(AcceptRemoteProgressOutcome {
            snapshot_id: prepared.selected_snapshot_id,
            safety_backup_created: backup_date.is_some(),
            manifest_revision,
        })
    }
}

/// Merge a Cloud lineage into the local catalog and return the local archives
/// that a verified Cloud copy superseded. The caller deletes them only after
/// the catalog is committed, so a failure never leaves a row without its file.
pub(crate) fn merge_remote_lineage(
    local: &mut GameSnapshots,
    lineage: &[Snapshot],
) -> Result<Vec<PathBuf>, CloudLibraryServiceError> {
    let mut superseded = Vec::new();
    for remote in lineage {
        if let Some(existing) = local
            .backups
            .iter_mut()
            .find(|snapshot| snapshot.date == remote.date)
        {
            if existing.parent != remote.parent || existing.created_by != remote.created_by {
                return Err(CloudLibraryServiceError::RemoteSnapshotIdentityConflict(
                    remote.date.clone(),
                ));
            }
            if archive_bytes_differ(existing, remote) {
                // Same history position with a differently packed archive, such as
                // one recompressed by a local format upgrade. Until the Cloud copy
                // is downloaded and verified, the local row stays untouched.
                superseded.extend(adopt_verified_cloud_archive(existing, remote));
                continue;
            }
            if std::path::Path::new(&remote.path).is_file() {
                existing.path = remote.path.clone();
            }
            existing.size = existing.size.max(remote.size);
            if existing.archive_hash.is_none() {
                existing.archive_hash = remote.archive_hash.clone();
            }
            if existing.describe.is_empty()
                && !local.pending_descriptions.contains_key(&remote.date)
            {
                existing.describe = remote.describe.clone();
            }
            existing.created_at = existing.created_at.or(remote.created_at);
            if existing.device_id.is_none() {
                existing.device_id = remote.device_id.clone();
            }
        } else {
            local.backups.push(remote.clone());
        }
    }
    Ok(superseded)
}

fn archive_bytes_differ(local: &Snapshot, remote: &Snapshot) -> bool {
    local.archive_format != remote.archive_format
        || (local.size > 0 && remote.size > 0 && local.size != remote.size)
        || matches!(
            (&local.archive_hash, &remote.archive_hash),
            (Some(local), Some(cloud)) if local != cloud
        )
}

/// Point the local row at the downloaded Cloud archive once its bytes match the
/// Cloud integrity record, and return the local archive it replaces.
fn adopt_verified_cloud_archive(existing: &mut Snapshot, remote: &Snapshot) -> Option<PathBuf> {
    let remote_path = PathBuf::from(&remote.path);
    let integrity = ArchiveIntegrity {
        size: remote.size,
        xxh3_64: remote.archive_hash.clone()?,
    };
    integrity.verify_file(&remote_path).ok()?;
    let replaced = snapshot_archive_path(remote_path.parent()?, existing);
    existing.path = remote.path.clone();
    existing.archive_name = remote.archive_name.clone();
    existing.archive_format = remote.archive_format;
    existing.size = remote.size;
    existing.archive_hash = remote.archive_hash.clone();
    (replaced != remote_path).then_some(replaced)
}

pub(crate) fn remove_superseded_archives(paths: &[PathBuf]) {
    for path in paths {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => log::warn!(
                "Could not remove the replaced local archive {}: {error}",
                path.display()
            ),
        }
    }
}

async fn rollback_remote_progress(
    services: &ServiceContext,
    game: &Game,
    original: &GameSnapshots,
    lineage: &[Snapshot],
    backup_date: Option<&str>,
) -> String {
    let mut failures = Vec::new();
    match backup_date {
        Some(date) => {
            if let Err(error) = services.restore_extra_backup(game, date, None).await {
                failures.push(format!("live save: {error}"));
            }
        }
        None => failures.push("live save: no prior save data was available to archive".to_string()),
    }
    // Undo only this operation's changes; snapshots committed meanwhile stay.
    let rolled_back = game.update_game_snapshots_info::<crate::preclude::BackupError>(|current| {
        current.backups.retain(|snapshot| {
            original
                .backups
                .iter()
                .any(|kept| kept.date == snapshot.date)
                || !lineage.iter().any(|added| added.date == snapshot.date)
        });
        for snapshot in &mut current.backups {
            if let Some(before) = original
                .backups
                .iter()
                .find(|before| before.date == snapshot.date)
            {
                *snapshot = before.clone();
            }
        }
        current.set_current_device_head(original.current_device_head().cloned());
        Ok(())
    });
    if let Err(error) = rolled_back {
        failures.push(format!("Snapshot metadata: {error}"));
    }
    if failures.is_empty() {
        "completed".to_string()
    } else {
        failures.join("; ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{ArchiveFormat, CreatedBy, archive_path};

    fn snapshot(id: &str, parent: Option<&str>) -> Snapshot {
        Snapshot {
            date: id.into(),
            describe: id.into(),
            path: String::new(),
            archive_name: None,
            archive_format: ArchiveFormat::Zip,
            size: 5,
            parent: parent.map(str::to_string),
            archive_hash: Some("0000000000000000".into()),
            created_at: None,
            device_id: None,
            created_by: CreatedBy::Manual,
        }
    }

    #[test]
    fn remote_lineage_is_merged_without_moving_any_local_head() {
        let mut local = GameSnapshots::new("Game");
        local.backups.push(snapshot("local", None));
        local.set_head_for_device("pc".into(), Some("local".into()));

        merge_remote_lineage(
            &mut local,
            &[snapshot("root", None), snapshot("remote", Some("root"))],
        )
        .unwrap();

        assert_eq!(local.head_for_device(&"pc".to_string()).unwrap(), "local");
        assert_eq!(local.backups.len(), 3);
    }

    #[test]
    fn remote_lineage_merges_creation_metadata_without_relabeling_known_origin() {
        let mut local = GameSnapshots::new("Game");
        local.backups.push(snapshot("same", None));
        let mut remote = snapshot("same", None);
        remote.created_at = Some(1234);
        remote.device_id = Some("original".into());
        merge_remote_lineage(&mut local, &[remote.clone()]).unwrap();
        assert_eq!(local.backups[0].created_at, Some(1234));
        assert_eq!(local.backups[0].device_id.as_deref(), Some("original"));
        remote.created_at = Some(5678);
        remote.device_id = Some("holder".into());
        merge_remote_lineage(&mut local, &[remote]).unwrap();
        assert_eq!(local.backups[0].created_at, Some(1234));
        assert_eq!(local.backups[0].device_id.as_deref(), Some("original"));
    }

    fn archive_snapshot(
        dir: &std::path::Path,
        id: &str,
        format: ArchiveFormat,
        name: Option<&str>,
        bytes: &[u8],
    ) -> Snapshot {
        let path = archive_path(dir, id, format, name);
        std::fs::write(&path, bytes).unwrap();
        let integrity = ArchiveIntegrity::from_file(&path).unwrap();
        Snapshot {
            path: path.to_string_lossy().into_owned(),
            archive_name: name.map(str::to_string),
            archive_format: format,
            size: integrity.size,
            archive_hash: Some(integrity.xxh3_64),
            ..snapshot(id, None)
        }
    }

    #[test]
    fn verified_cloud_archive_replaces_a_recompressed_local_copy() {
        let temp = temp_dir::TempDir::new().unwrap();
        let local_row = archive_snapshot(
            temp.path(),
            "same",
            ArchiveFormat::SevenZ,
            Some("same_abc.7z"),
            b"recompressed",
        );
        let cloud_row =
            archive_snapshot(temp.path(), "same", ArchiveFormat::Zip, None, b"original");
        let mut local = GameSnapshots::new("Game");
        local.backups.push(local_row.clone());

        let superseded =
            merge_remote_lineage(&mut local, std::slice::from_ref(&cloud_row)).unwrap();

        assert_eq!(local.backups.len(), 1);
        let merged = &local.backups[0];
        assert_eq!(merged.path, cloud_row.path);
        assert_eq!(merged.archive_name, None);
        assert_eq!(merged.archive_format, ArchiveFormat::Zip);
        assert_eq!(merged.size, cloud_row.size);
        assert_eq!(merged.archive_hash, cloud_row.archive_hash);
        assert_eq!(superseded, vec![PathBuf::from(&local_row.path)]);
        assert!(PathBuf::from(&local_row.path).is_file());
    }

    #[test]
    fn unverified_cloud_archive_leaves_the_local_copy_untouched() {
        let temp = temp_dir::TempDir::new().unwrap();
        let local_row = archive_snapshot(
            temp.path(),
            "same",
            ArchiveFormat::SevenZ,
            Some("same_abc.7z"),
            b"recompressed",
        );
        let mut cloud_row =
            archive_snapshot(temp.path(), "same", ArchiveFormat::Zip, None, b"original");
        let mut local = GameSnapshots::new("Game");
        local.backups.push(local_row.clone());

        std::fs::write(&cloud_row.path, b"corrupted").unwrap();
        assert!(
            merge_remote_lineage(&mut local, std::slice::from_ref(&cloud_row))
                .unwrap()
                .is_empty()
        );
        std::fs::remove_file(&cloud_row.path).unwrap();
        cloud_row.archive_hash = Some("0000000000000000".into());
        assert!(
            merge_remote_lineage(&mut local, &[cloud_row])
                .unwrap()
                .is_empty()
        );

        assert_eq!(local.backups, vec![local_row]);
    }

    #[test]
    fn different_creator_is_still_a_conflict() {
        let mut local = GameSnapshots::new("Game");
        local.backups.push(snapshot("same", None));
        let mut remote = snapshot("same", None);
        remote.created_by = CreatedBy::Timer;

        assert!(matches!(
            merge_remote_lineage(&mut local, &[remote]),
            Err(CloudLibraryServiceError::RemoteSnapshotIdentityConflict(_))
        ));
    }

    #[test]
    fn conflicting_snapshot_identity_fails_closed() {
        let mut local = GameSnapshots::new("Game");
        local.backups.push(snapshot("same", None));

        assert!(matches!(
            merge_remote_lineage(&mut local, &[snapshot("same", Some("other"))]),
            Err(CloudLibraryServiceError::RemoteSnapshotIdentityConflict(id)) if id == "same"
        ));
    }
}
