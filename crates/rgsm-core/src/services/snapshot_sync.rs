use std::collections::BTreeMap;
use std::sync::Arc;

use thiserror::Error;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::app_dirs::resolve_app_path;
use crate::backup::GameSnapshots;
use crate::cloud_sync::CloudSyncSessionConfig;
use crate::cloud_sync::v2::{
    SnapshotReconciliationOutcome, SnapshotSyncCoordinator, SnapshotSyncError,
};
use crate::config::{
    CloudNamespaceGeneration, Config, DeviceProfile, SyncMode, cloud_bootstrap_inputs, get_config,
};
use crate::hooks::{SnapshotSyncTarget, V2SnapshotSyncHook};
use crate::preclude::{BackendError, BackupError, ConfigError};

pub const DEFAULT_SNAPSHOT_SYNC_POLL_MINUTES: u64 = 5;

struct SnapshotSyncRuntime {
    coordinator: SnapshotSyncCoordinator,
    targets: BTreeMap<String, SnapshotSyncTarget>,
    game_names: BTreeMap<String, String>,
    config: Config,
}

pub fn build_v2_snapshot_sync_hook(
    operation_lock: Arc<Mutex<()>>,
) -> Result<Option<V2SnapshotSyncHook>, SnapshotSyncServiceError> {
    Ok(load_runtime()?.map(|runtime| {
        V2SnapshotSyncHook::new(runtime.coordinator, runtime.targets, operation_lock)
    }))
}

pub async fn run_v2_snapshot_sync_once(
    cancellation: &CancellationToken,
) -> Result<SnapshotReconciliationOutcome, SnapshotSyncServiceError> {
    let Some(runtime) = load_runtime()? else {
        return Ok(SnapshotReconciliationOutcome::default());
    };
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
            Some(game) => game.get_game_snapshots_info()?,
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
            .reconcile_game(
                game_id,
                &snapshots,
                target.activation_revision,
                &target.local_baseline,
                cancellation,
            )
            .await?;
        total.published += outcome.published;
        total.uploaded += outcome.uploaded;
        total.downloaded += outcome.downloaded;
    }
    Ok(total)
}

pub async fn resume_v2_snapshot_sync(
    cancellation: &CancellationToken,
) -> Result<usize, SnapshotSyncServiceError> {
    let Some(runtime) = load_runtime()? else {
        return Ok(0);
    };
    Ok(runtime.coordinator.resume_pending(cancellation).await?)
}

pub fn v2_snapshot_sync_poll_minutes() -> Result<Option<u64>, SnapshotSyncServiceError> {
    let (_, profile, local_state) = cloud_bootstrap_inputs()?;
    if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2
        || !profile
            .games
            .values()
            .any(|game| game.sync_mode == SyncMode::SnapshotSync)
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

fn load_runtime() -> Result<Option<SnapshotSyncRuntime>, SnapshotSyncServiceError> {
    let (library, profile, local_state) = cloud_bootstrap_inputs()?;
    if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
        return Ok(None);
    }
    let targets = sync_targets(&profile);
    if targets.is_empty() {
        return Ok(None);
    }
    let archive_root = profile
        .local_archive_root
        .as_deref()
        .map(resolve_app_path)
        .ok_or(SnapshotSyncServiceError::StorageLocationRequired)?;
    let session = CloudSyncSessionConfig::from(&local_state.cloud_settings);
    Ok(Some(SnapshotSyncRuntime {
        coordinator: SnapshotSyncCoordinator::new(
            session.get_op()?,
            archive_root,
            local_state.current_device_id,
            resolve_app_path("GameSaveManager.cloud-v2-materialization.json"),
            3,
        ),
        targets,
        game_names: library
            .games
            .into_iter()
            .map(|game| (game.storage_key, game.name))
            .collect(),
        config: get_config()?,
    }))
}

fn sync_targets(profile: &DeviceProfile) -> BTreeMap<String, SnapshotSyncTarget> {
    profile
        .games
        .iter()
        .filter_map(|(game_id, settings)| {
            (settings.sync_mode == SyncMode::SnapshotSync).then(|| {
                settings.snapshot_sync_activation_revision.map(|revision| {
                    (
                        game_id.clone(),
                        SnapshotSyncTarget {
                            activation_revision: revision,
                            local_baseline: settings.snapshot_sync_local_baseline.clone(),
                        },
                    )
                })
            })?
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DeviceGameProfile, InitialCatchUpPolicy};

    #[test]
    fn only_snapshot_sync_profiles_with_boundaries_become_targets() {
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
        let game = |mode, revision| DeviceGameProfile {
            visible: true,
            sync_mode: mode,
            snapshot_sync_activation_revision: revision,
            snapshot_sync_local_baseline: Default::default(),
            initial_catch_up: InitialCatchUpPolicy::KeepRemote,
            game_path: None,
            binding: None,
            auto_backup: None,
            save_units: Default::default(),
        };
        profile
            .games
            .insert("manual".into(), game(SyncMode::Manual, None));
        profile
            .games
            .insert("ready".into(), game(SyncMode::SnapshotSync, Some(4)));
        profile
            .games
            .insert("incomplete".into(), game(SyncMode::SnapshotSync, None));

        let targets = sync_targets(&profile);

        assert_eq!(targets.keys().cloned().collect::<Vec<_>>(), vec!["ready"]);
        assert_eq!(targets["ready"].activation_revision, 4);
    }
}
