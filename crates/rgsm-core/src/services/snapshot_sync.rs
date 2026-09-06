use std::collections::BTreeMap;
use std::sync::Arc;

use thiserror::Error;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::app_dirs::resolve_app_path;
use crate::backup::GameSnapshots;
use crate::cloud_sync::v2::{
    CloudLibraryTarget, SnapshotReconciliationOutcome, SnapshotSyncCoordinator, SnapshotSyncError,
};
use crate::config::{
    CloudNamespaceGeneration, Config, DeviceProfile, cloud_bootstrap_inputs, get_config,
};
use crate::hooks::{SnapshotSyncTarget, V2SnapshotSyncHook};
use crate::preclude::{BackendError, BackupError, ConfigError};

use super::cloud_library_target::cloud_library_target;

pub const DEFAULT_SNAPSHOT_SYNC_POLL_MINUTES: u64 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSaveSyncTarget {
    pub game_id: String,
    pub process_name: String,
    pub snapshot_on_exit: bool,
}

struct SnapshotSyncRuntime {
    target: CloudLibraryTarget,
    coordinator: SnapshotSyncCoordinator,
    targets: BTreeMap<String, SnapshotSyncTarget>,
    game_names: BTreeMap<String, String>,
    config: Config,
}

pub fn build_v2_snapshot_sync_hook(
    operation_lock: Arc<Mutex<()>>,
) -> Result<Option<V2SnapshotSyncHook>, SnapshotSyncServiceError> {
    Ok(load_runtime()?.map(|runtime| {
        V2SnapshotSyncHook::new(
            runtime.target,
            runtime.coordinator,
            runtime.targets,
            operation_lock,
        )
    }))
}

pub async fn run_v2_snapshot_sync_once(
    cancellation: &CancellationToken,
) -> Result<SnapshotReconciliationOutcome, SnapshotSyncServiceError> {
    super::game_deletion::converge_local_deleted_games().await?;
    super::cloud_library_metadata::refresh_shared_library().await?;
    let Some(runtime) = load_runtime()? else {
        return Ok(SnapshotReconciliationOutcome::default());
    };
    runtime.target.verify().await.map_err(BackendError::from)?;
    let tombstones = runtime.coordinator.converge_local_tombstones().await?;
    let mut total = SnapshotReconciliationOutcome::default();
    for (game_id, target) in &runtime.targets {
        if cancellation.is_cancelled() {
            return Err(SnapshotSyncServiceError::Cancelled);
        }
        let snapshots = match runtime
            .config
            .games
            .iter()
            .find(|game| game.storage_key == *game_id)
        {
            Some(game) => {
                if let Some(snapshot_ids) = tombstones.get(game_id) {
                    game.forget_v2_tombstones(snapshot_ids)?;
                }
                game.get_game_snapshots_info()?
            }
            None => GameSnapshots::new(
                runtime
                    .game_names
                    .get(game_id)
                    .cloned()
                    .unwrap_or_else(|| game_id.clone()),
            ),
        };

        let outcome = runtime
            .coordinator
            .reconcile_game_with_policy(
                game_id,
                &snapshots,
                target.activation_revision,
                &target.local_baseline,
                cancellation,
                crate::cloud_sync::v2::SnapshotReconcilePolicy {
                    upload_new_archives: target.upload_new_archives,
                },
            )
            .await?;
        total.published += outcome.published;
        total.uploaded += outcome.uploaded;
        if let Some(limit) = target.retention_limit {
            let retention = runtime
                .coordinator
                .enforce_retention(game_id, limit)
                .await?;
            if let Some(game) = runtime
                .config
                .games
                .iter()
                .find(|game| game.storage_key == *game_id)
            {
                game.forget_v2_tombstones(&retention.tombstones)?;
            }
        }
    }
    Ok(total)
}

pub async fn resume_v2_snapshot_sync(
    cancellation: &CancellationToken,
) -> Result<usize, SnapshotSyncServiceError> {
    super::game_deletion::converge_local_deleted_games().await?;
    let Some(runtime) = load_runtime()? else {
        return Ok(0);
    };
    runtime.target.verify().await.map_err(BackendError::from)?;
    let downloaded = runtime.coordinator.resume_pending(cancellation).await;
    let import =
        super::sync::import_local_verified_catalog(runtime.coordinator.materializer()).await;
    let downloaded = downloaded?;
    import?;
    Ok(downloaded)
}

pub fn v2_snapshot_sync_poll_minutes() -> Result<Option<u64>, SnapshotSyncServiceError> {
    let (_, profile, local_state) = cloud_bootstrap_inputs()?;
    if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2
        || !profile.games.values().any(|game| game.cloud_sync_enabled)
    {
        return Ok(None);
    }
    let configured = local_state.cloud_settings.auto_sync_interval;
    Ok(Some(if configured == 0 {
        DEFAULT_SNAPSHOT_SYNC_POLL_MINUTES
    } else {
        configured
    }))
}

pub fn v2_live_save_sync_targets() -> Result<Vec<LiveSaveSyncTarget>, SnapshotSyncServiceError> {
    let (_, profile, local_state) = cloud_bootstrap_inputs()?;
    if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
        return Ok(Vec::new());
    }
    Ok(profile
        .games
        .into_iter()
        .filter_map(|(game_id, settings)| {
            if !settings.cloud_sync_enabled || !settings.sync_mode.checks_remote_progress() {
                return None;
            }
            let process_name = settings.live_save_process_name.unwrap_or_default();
            if process_name.is_empty() {
                // Process-exit capture needs a process; cloud sync itself does not.
                return None;
            }
            Some(LiveSaveSyncTarget {
                game_id,
                process_name,
                snapshot_on_exit: settings.live_save_snapshot_on_exit,
            })
        })
        .collect())
}

fn load_runtime() -> Result<Option<SnapshotSyncRuntime>, SnapshotSyncServiceError> {
    let (library, profile, local_state) = cloud_bootstrap_inputs()?;
    if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
        return Ok(None);
    }
    let mut targets = sync_targets(&profile, &library);
    targets.retain(|game_id, _| !local_state.is_local_game(game_id));
    if targets.is_empty() {
        return Ok(None);
    }
    let archive_root = profile
        .local_archive_root
        .as_deref()
        .map(resolve_app_path)
        .ok_or(SnapshotSyncServiceError::StorageLocationRequired)?;
    let target = cloud_library_target(&local_state)?;
    let excluded = local_state.local_game_ids();
    Ok(Some(SnapshotSyncRuntime {
        target: target.clone(),
        coordinator: SnapshotSyncCoordinator::new(
            target.operator(),
            archive_root,
            local_state.current_device_id,
            resolve_app_path("GameSaveManager.cloud-v2-materialization.json"),
            3,
        )
        .excluding_games(excluded),
        targets,
        game_names: library
            .games
            .into_iter()
            .map(|game| (game.storage_key, game.name))
            .collect(),
        config: get_config()?,
    }))
}

fn sync_targets(
    profile: &DeviceProfile,
    library: &crate::config::SharedLibrary,
) -> BTreeMap<String, SnapshotSyncTarget> {
    profile
        .games
        .iter()
        .filter_map(|(game_id, settings)| {
            if !settings.cloud_sync_enabled {
                return None;
            }
            settings.snapshot_sync_activation_revision.map(|revision| {
                (
                    game_id.clone(),
                    SnapshotSyncTarget {
                        activation_revision: revision,
                        local_baseline: settings.snapshot_sync_local_baseline.clone(),
                        retention_limit: library
                            .games
                            .iter()
                            .find(|game| game.storage_key == *game_id)
                            .and_then(|game| game.snapshot_retention)
                            .map(|policy| policy.automatic_snapshots_per_branch),
                        upload_new_archives: settings.sync_mode.auto_uploads_archives(),
                    },
                )
            })
        })
        .collect()
}

#[derive(Debug, Error)]
pub enum SnapshotSyncServiceError {
    #[error("Choose a local archive folder before using Snapshot Sync")]
    StorageLocationRequired,
    #[error("Snapshot Sync was cancelled")]
    Cancelled,
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error(transparent)]
    Backup(#[from] BackupError),
    #[error(transparent)]
    SnapshotSync(#[from] SnapshotSyncError),
    #[error(transparent)]
    Retention(#[from] super::CloudLibraryServiceError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DeviceGameProfile, InitialCatchUpPolicy, SyncMode};

    #[test]
    fn synchronized_profiles_with_boundaries_become_targets() {
        let mut profile = DeviceProfile {
            schema_version: crate::config::V2_CONFIG_SCHEMA_VERSION,
            device: crate::device::Device {
                id: "deck".into(),
                name: "Deck".into(),
                resources: Vec::new(),
                next_resource_id: 0,
            },
            local_archive_root: None,
            games: Default::default(),
            private_favorites: Vec::new(),
            quick_action: Default::default(),
            behavior: serde_json::from_value(serde_json::json!({
                "prompt_when_not_described": false,
                "extra_backup_when_apply": false,
                "confirm_before_apply_latest": true,
                "confirm_before_apply_snapshot": true,
                "prompt_when_auto_backup": false,
                "default_delete_before_apply": false,
                "add_new_to_favorites": false,
                "vn_scan_dirs": [],
                "max_auto_backup_count": 0,
                "max_extra_backup_count": 0,
                "compression_preset": "Fast",
                "compute_archive_hash": false,
                "verify_archive_before_apply": false
            }))
            .unwrap(),
        };
        let game = |mode, revision: Option<u64>| DeviceGameProfile {
            visible: true,
            cloud_sync_enabled: revision.is_some(),
            sync_mode: mode,
            snapshot_sync_activation_revision: revision,
            snapshot_sync_local_baseline: Default::default(),
            initial_catch_up: InitialCatchUpPolicy::KeepRemote,
            live_save_process_name: None,
            live_save_snapshot_on_exit: false,
            game_path: None,
            binding: None,
            auto_backup: None,
            save_units: Default::default(),
        };
        profile
            .games
            .insert("manual".into(), game(SyncMode::Manual, Some(3)));
        profile
            .games
            .insert("ready".into(), game(SyncMode::CloudBackup, Some(4)));
        profile
            .games
            .insert("live".into(), game(SyncMode::MultiDeviceSync, Some(5)));
        profile
            .games
            .insert("incomplete".into(), game(SyncMode::CloudBackup, None));

        let targets = sync_targets(
            &profile,
            &crate::config::SharedLibrary {
                schema_version: crate::config::V2_CONFIG_SCHEMA_VERSION,
                games: Vec::new(),
            },
        );

        assert_eq!(
            targets.keys().cloned().collect::<Vec<_>>(),
            vec!["live", "manual", "ready"]
        );
        assert_eq!(targets["live"].activation_revision, 5);
        assert_eq!(targets["ready"].activation_revision, 4);
        assert!(!targets["manual"].upload_new_archives);
        assert!(targets["ready"].upload_new_archives);
        assert!(targets["live"].upload_new_archives);

        let mut legacy = serde_json::to_value(game(SyncMode::MultiDeviceSync, Some(5))).unwrap();
        legacy["multi_device_sync_suspended"] = serde_json::json!(true);
        profile
            .games
            .insert("live".into(), serde_json::from_value(legacy).unwrap());
        let resumed = sync_targets(
            &profile,
            &crate::config::SharedLibrary {
                schema_version: crate::config::V2_CONFIG_SCHEMA_VERSION,
                games: Vec::new(),
            },
        );
        assert!(resumed["live"].upload_new_archives);
        assert!(profile.games["live"].sync_mode.checks_remote_progress());
    }
}
