use serde::Serialize;
use specta::Type;

use crate::app_dirs::resolve_app_path;
use crate::backup::{CapturePlanError, Game, GameSnapshots, Snapshot};
use crate::cloud_sync::CloudSyncSessionConfig;
use crate::cloud_sync::v2::{
    KeepLocalProgressOutcome, V2ConflictResolver, V2RemoteProgressResolver,
};
use crate::config::{
    CloudNamespaceGeneration, cloud_bootstrap_inputs, get_config, resolve_backup_path,
};
use crate::hooks::HookSource;

use super::{CloudLibraryServiceError, ServiceContext};

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
        let session = CloudSyncSessionConfig::from(&local_state.cloud_settings);
        Ok(V2ConflictResolver::new(
            session.get_op()?,
            local_archive_root,
            local_state.current_device_id,
            resolve_app_path("GameSaveManager.cloud-v2-materialization.json"),
            3,
        )
        .keep_local(game_id, manifest_revision, local_snapshot_id, &local)
        .await?)
    }

    pub async fn accept_v2_remote_progress(
        &self,
        game_id: &str,
        manifest_revision: u64,
        expected_local_snapshot_id: Option<&str>,
        selected_snapshot_id: &str,
    ) -> Result<AcceptRemoteProgressOutcome, CloudLibraryServiceError> {
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
        let session = CloudSyncSessionConfig::from(&local_state.cloud_settings);
        let resolver = V2RemoteProgressResolver::new(
            session.get_op()?,
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

        let latest = game.get_game_snapshots_info()?;
        if latest != original {
            return Err(CloudLibraryServiceError::LocalSnapshotHistoryChanged);
        }
        let mut staged = latest;
        merge_remote_lineage(&mut staged, &prepared.lineage)?;
        let backup_date = match self.capture_plan(&config, &game) {
            Ok(plan) => Some(game.create_overwrite_snapshot_from_capture_plan(
                &plan,
                &resolve_backup_path(&config.backup_path),
                config.settings.compression_preset,
                config.settings.max_extra_backup_count,
            )?),
            Err(CapturePlanError::NoDataMatched) => None,
            Err(error) => return Err(crate::preclude::BackupError::from(error).into()),
        };

        game.set_game_snapshots_info(&staged)?;
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
                rollback: rollback_remote_progress(self, &game, &original, backup_date.as_deref()),
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
                        backup_date.as_deref(),
                    ),
                });
            }
        };
        Ok(AcceptRemoteProgressOutcome {
            snapshot_id: prepared.selected_snapshot_id,
            safety_backup_created: backup_date.is_some(),
            manifest_revision,
        })
    }
}

pub(crate) fn merge_remote_lineage(
    local: &mut GameSnapshots,
    lineage: &[Snapshot],
) -> Result<(), CloudLibraryServiceError> {
    for remote in lineage {
        if let Some(existing) = local
            .backups
            .iter_mut()
            .find(|snapshot| snapshot.date == remote.date)
        {
            if existing.parent != remote.parent
                || existing.archive_format != remote.archive_format
                || existing.created_by != remote.created_by
                || (existing.size > 0 && remote.size > 0 && existing.size != remote.size)
                || matches!(
                    (&existing.archive_hash, &remote.archive_hash),
                    (Some(local), Some(cloud)) if local != cloud
                )
            {
                return Err(CloudLibraryServiceError::RemoteSnapshotIdentityConflict(
                    remote.date.clone(),
                ));
            }
            if std::path::Path::new(&remote.path).is_file() {
                existing.path = remote.path.clone();
            }
            existing.size = existing.size.max(remote.size);
            if existing.archive_hash.is_none() {
                existing.archive_hash = remote.archive_hash.clone();
            }
            if existing.describe.is_empty() {
                existing.describe = remote.describe.clone();
            }
        } else {
            local.backups.push(remote.clone());
        }
    }
    Ok(())
}

fn rollback_remote_progress(
    services: &ServiceContext,
    game: &Game,
    original: &GameSnapshots,
    backup_date: Option<&str>,
) -> String {
    let mut failures = Vec::new();
    match backup_date {
        Some(date) => {
            if let Err(error) = services.restore_extra_backup(game, date, None) {
                failures.push(format!("live save: {error}"));
            }
        }
        None => failures.push("live save: no prior save data was available to archive".to_string()),
    }
    if let Err(error) = game.set_game_snapshots_info(original) {
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
    use crate::backup::{ArchiveFormat, CreatedBy};

    fn snapshot(id: &str, parent: Option<&str>) -> Snapshot {
        Snapshot {
            date: id.into(),
            describe: id.into(),
            path: String::new(),
            archive_format: ArchiveFormat::Zip,
            size: 5,
            parent: parent.map(str::to_string),
            archive_hash: Some("0000000000000000".into()),
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
    fn conflicting_snapshot_identity_fails_closed() {
        let mut local = GameSnapshots::new("Game");
        local.backups.push(snapshot("same", None));

        assert!(matches!(
            merge_remote_lineage(&mut local, &[snapshot("same", Some("other"))]),
            Err(CloudLibraryServiceError::RemoteSnapshotIdentityConflict(id)) if id == "same"
        ));
    }
}
