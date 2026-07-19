use std::collections::BTreeMap;
use std::sync::Arc;

use thiserror::Error;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::app_dirs::resolve_app_path;
use crate::backup::GameSnapshots;
use crate::cloud_sync::CloudSyncSessionConfig;
use crate::cloud_sync::v2::{
    ProgressRelation, SnapshotReconciliationOutcome, SnapshotSyncCoordinator, SnapshotSyncError,
    V2ConflictInspector, V2ConflictReview,
};
use crate::config::{
    CloudNamespaceGeneration, Config, DeviceProfile, SyncMode, cloud_bootstrap_inputs, get_config,
};
use crate::hooks::{SnapshotSyncTarget, V2SnapshotSyncHook};
use crate::preclude::{BackendError, BackupError, ConfigError};

pub const DEFAULT_SNAPSHOT_SYNC_POLL_MINUTES: u64 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSaveSyncTarget {
    pub game_id: String,
    pub process_name: String,
    pub snapshot_on_exit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveSaveApplyPlan {
    pub game_id: String,
    pub manifest_revision: u64,
    pub expected_local_snapshot_id: Option<String>,
    pub selected_snapshot_id: String,
}

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
            .any(|game| game.sync_mode != SyncMode::Manual)
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
            if settings.sync_mode != SyncMode::LiveSaveSync {
                return None;
            }
            Some(LiveSaveSyncTarget {
                game_id,
                process_name: settings.live_save_process_name?,
                snapshot_on_exit: settings.live_save_snapshot_on_exit,
            })
        })
        .collect())
}

pub async fn review_v2_live_save_apply(
    game_id: &str,
) -> Result<Option<LiveSaveApplyPlan>, SnapshotSyncServiceError> {
    let (_, profile, local_state) = cloud_bootstrap_inputs()?;
    if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2
        || profile
            .games
            .get(game_id)
            .is_none_or(|settings| settings.sync_mode != SyncMode::LiveSaveSync)
    {
        return Ok(None);
    }
    let config = get_config()?;
    let Some(game) = config.games.iter().find(|game| game.storage_key == game_id) else {
        return Ok(None);
    };
    let local = game.get_game_snapshots_info()?;
    let archive_root = profile
        .local_archive_root
        .as_deref()
        .map(resolve_app_path)
        .ok_or(SnapshotSyncServiceError::StorageLocationRequired)?;
    let review = V2ConflictInspector::new(
        CloudSyncSessionConfig::from(&local_state.cloud_settings).get_op()?,
        archive_root,
        local_state.current_device_id.clone(),
        3,
    )
    .review(game_id, &local)
    .await?;
    Ok(select_live_save_apply(
        review,
        &local_state.current_device_id,
    ))
}

/// Selects only the single fail-safe graph transition that Live Save Sync may
/// apply without asking the user. Any additional remote Head, unavailable
/// archive, or relation other than remote-ahead/no-local-position is ambiguous
/// and deliberately yields no plan.
fn select_live_save_apply(
    review: V2ConflictReview,
    current_device_id: &str,
) -> Option<LiveSaveApplyPlan> {
    let mut remote = review.candidates.iter().filter(|candidate| {
        candidate
            .devices
            .iter()
            .any(|device| device != current_device_id)
    });
    let selected = remote.next()?;
    if remote.next().is_some()
        || !selected.cloud_available
        || !matches!(
            selected.relation,
            ProgressRelation::RemoteAhead | ProgressRelation::NoLocalPosition
        )
    {
        return None;
    }
    Some(LiveSaveApplyPlan {
        game_id: review.game_id,
        manifest_revision: review.manifest_revision,
        expected_local_snapshot_id: review.local.map(|local| local.snapshot_id),
        selected_snapshot_id: selected.snapshot_id.clone(),
    })
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
            (settings.sync_mode != SyncMode::Manual).then(|| {
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
    #[error(transparent)]
    ConflictReview(#[from] crate::cloud_sync::v2::ConflictReviewError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud_sync::v2::{LocalProgressView, RemoteProgressCandidate};
    use crate::config::{DeviceGameProfile, InitialCatchUpPolicy};

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
        let game = |mode, revision| DeviceGameProfile {
            visible: true,
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
            .insert("manual".into(), game(SyncMode::Manual, None));
        profile
            .games
            .insert("ready".into(), game(SyncMode::SnapshotSync, Some(4)));
        profile
            .games
            .insert("live".into(), game(SyncMode::LiveSaveSync, Some(5)));
        profile
            .games
            .insert("incomplete".into(), game(SyncMode::SnapshotSync, None));

        let targets = sync_targets(&profile);

        assert_eq!(
            targets.keys().cloned().collect::<Vec<_>>(),
            vec!["live", "ready"]
        );
        assert_eq!(targets["live"].activation_revision, 5);
        assert_eq!(targets["ready"].activation_revision, 4);
    }

    fn review(candidates: Vec<RemoteProgressCandidate>) -> V2ConflictReview {
        V2ConflictReview {
            game_id: "game".into(),
            manifest_revision: 9,
            local: Some(LocalProgressView {
                snapshot_id: "local".into(),
                description: String::new(),
                local_available: true,
                cloud_available: true,
            }),
            candidates,
            requires_choice: true,
        }
    }

    fn candidate(id: &str, device: &str, relation: ProgressRelation) -> RemoteProgressCandidate {
        RemoteProgressCandidate {
            snapshot_id: id.into(),
            description: String::new(),
            devices: vec![device.into()],
            relation,
            local_unique_snapshots: 0,
            remote_unique_snapshots: 1,
            common_ancestor: Some("root".into()),
            local_available: false,
            cloud_available: true,
        }
    }

    #[test]
    fn automatic_apply_accepts_one_remote_ahead_candidate() {
        let plan = select_live_save_apply(
            review(vec![candidate(
                "remote",
                "deck",
                ProgressRelation::RemoteAhead,
            )]),
            "pc",
        )
        .unwrap();

        assert_eq!(plan.selected_snapshot_id, "remote");
        assert_eq!(plan.expected_local_snapshot_id.as_deref(), Some("local"));
    }

    #[test]
    fn automatic_apply_never_chooses_between_multiple_or_divergent_heads() {
        assert!(
            select_live_save_apply(
                review(vec![
                    candidate("older", "deck", ProgressRelation::RemoteAhead),
                    candidate("newer", "laptop", ProgressRelation::RemoteAhead),
                ]),
                "pc",
            )
            .is_none()
        );
        assert!(
            select_live_save_apply(
                review(vec![candidate(
                    "fork",
                    "deck",
                    ProgressRelation::DifferentProgress,
                )]),
                "pc",
            )
            .is_none()
        );
    }
}
