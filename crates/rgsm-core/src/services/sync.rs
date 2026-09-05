use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::app_dirs::resolve_app_path;
use crate::backup::{Game, GameSnapshots};
use crate::cloud_sync::v2::{
    AcceptRemoteProgressError, CLOUD_MANIFEST_PATH, CloudArchiveEvictionError,
    CloudArchiveMaterializer, CloudLibraryBootstrap, CloudLibraryBootstrapError,
    CloudLibraryCutover, CloudLibraryCutoverError, CloudLibraryCutoverReview, CloudLibraryJoin,
    CloudLibraryJoinError, CloudLibraryJoinReview, CloudManifestRepository,
    CloudNamespaceClassification, CloudNamespaceDescriptor, CloudNamespaceError,
    ConflictReviewError, DeletionRegistryError, DeviceProfileRemovalError, DeviceProfileRepository,
    DeviceProfileRepositoryError, JoinGameDecision, KeepLocalProgressError,
    LocalArchiveEvictionError, ManifestRepositoryError, MaterializationError,
    MaterializationOutcome, MaterializationPreview, SharedGameDeletionError,
    SharedLibraryRepositoryError, SnapshotDeletionLifecycleError, SnapshotReconcilePolicy,
    SnapshotSyncCoordinator, SnapshotSyncError, V2ConflictInspector, V2ConflictReview,
};
use crate::cloud_sync::{
    BatchSyncReport, CloudBackendCheckReport, CloudSyncSessionConfig, ConflictResolution,
    ConflictResolutionOutcome, SyncGameOutcome, download_all_from_session,
    resolve_game_conflict as resolve_cloud_conflict, sync_config as sync_cloud_config,
    sync_game as sync_cloud_game, upload_all_from_session,
};
use crate::config::{
    CloudNamespaceGeneration, Config, InitialCatchUpPolicy, SyncMode, activate_cloud_namespace_v2,
    activate_cutover_cloud_library, activate_joined_cloud_library, cloud_bootstrap_inputs,
    cloud_namespace_generation, get_config, replace_current_device_profile, resolve_backup_path,
};
use crate::hooks::{HookSource, MetadataChangedCtx};
use crate::preclude::BackendError;

use super::{ServiceContext, cloud_library_target::bound_v2_operator};

fn cloud_session(config: &Config) -> CloudSyncSessionConfig {
    CloudSyncSessionConfig::from(&config.settings.cloud_settings)
}

fn find_game(config: &Config, game_name: &str) -> Result<Game, BackendError> {
    config
        .games
        .iter()
        .find(|game| game.name == game_name)
        .cloned()
        .ok_or_else(|| BackendError::GameNotFound(game_name.to_string()))
}

fn ensure_legacy_cloud_sync() -> Result<(), BackendError> {
    if cloud_namespace_generation()? == CloudNamespaceGeneration::V2 {
        Err(BackendError::V2CloudLibraryActive)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CloudLibraryStatus {
    Empty,
    JoinRequired { game_count: usize },
    ReconnectRequired { game_count: usize },
    RebuildRequired,
    CutoverRequired { game_count: usize, resumable: bool },
    Active { game_count: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CloudLibraryJoinOutcome {
    Active { game_count: usize },
    ReviewChanged { game_name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct CloudLibraryCutoverOutcome {
    pub game_count: usize,
    pub snapshot_count: usize,
    pub unavailable_archives: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct GameSyncModeOutcome {
    pub mode: SyncMode,
    pub cloud_sync_enabled: bool,
    pub published: usize,
    pub downloaded: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CurrentPositionDecision {
    Apply {
        snapshot_id: String,
    },
    Capture,
    Clear,
    /// Fall back to the deleted Snapshot's parent. No file restore, no new
    /// capture — the local save files stay as they are.
    FallbackToParent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct LiveSaveSyncOptions {
    pub process_name: String,
    pub snapshot_on_exit: bool,
}

#[derive(Debug, Error)]
pub enum CloudLibraryServiceError {
    #[error("Cloud Library creation requires explicit confirmation")]
    ConfirmationRequired,
    #[error("This device's Current Position still points at {0}")]
    CurrentPositionBlocksDeletion(String),
    #[error("The active V2 Cloud Library no longer matches the saved connection")]
    ActiveLibraryUnavailable,
    #[error("This device must reconnect to the rebuilt Cloud Library")]
    DeviceReconnectRequired,
    #[error("This installation does not need to join a Cloud Library")]
    JoinNotRequired,
    #[error("This installation does not need Cloud Cutover")]
    CutoverNotRequired,
    #[error("Enabling or reducing shared Snapshot retention requires explicit confirmation")]
    RetentionConfirmationRequired,
    #[error("Removing Snapshot retention protection requires explicit confirmation")]
    ProtectionRemovalConfirmationRequired,
    #[error("Snapshot retention must keep at least one automatic Snapshot per Branch")]
    InvalidRetentionLimit,
    #[error(transparent)]
    Config(#[from] crate::preclude::ConfigError),
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error(transparent)]
    Backup(#[from] crate::preclude::BackupError),
    #[error(transparent)]
    Bootstrap(#[from] CloudLibraryBootstrapError),
    #[error(transparent)]
    Join(#[from] CloudLibraryJoinError),
    #[error(transparent)]
    Cutover(#[from] CloudLibraryCutoverError),
    #[error(transparent)]
    Materialization(#[from] MaterializationError),
    #[error(transparent)]
    ConflictReview(#[from] ConflictReviewError),
    #[error(transparent)]
    KeepLocalProgress(#[from] KeepLocalProgressError),
    #[error(transparent)]
    AcceptRemoteProgress(#[from] AcceptRemoteProgressError),
    #[error(transparent)]
    DeviceProfile(#[from] DeviceProfileRepositoryError),
    #[error(transparent)]
    SharedLibrary(#[from] SharedLibraryRepositoryError),
    #[error(transparent)]
    ManifestRepository(#[from] ManifestRepositoryError),
    #[error(transparent)]
    SnapshotSync(#[from] SnapshotSyncError),
    #[error(transparent)]
    LocalArchiveEviction(#[from] LocalArchiveEvictionError),
    #[error(transparent)]
    CloudArchiveEviction(#[from] CloudArchiveEvictionError),
    #[error(transparent)]
    DeletionRegistry(#[from] DeletionRegistryError),
    #[error(transparent)]
    DeviceProfileRemoval(#[from] DeviceProfileRemovalError),
    #[error(transparent)]
    SharedGameDeletion(#[from] SharedGameDeletionError),
    #[error("Game is not configured on this device: {0}")]
    GameProfileNotFound(String),
    #[error("Game is not managed on this device: {0}")]
    GameNotManaged(String),
    #[error("Live Save Sync requires a process name")]
    LiveSaveProcessRequired,
    #[error("Choose a local archive folder before downloading or uploading Snapshots")]
    StorageLocationRequired,
    #[error("Cloud Cutover progress identity could not be created: {0}")]
    CutoverProgressIdentity(#[from] serde_json::Error),
    #[error("Remote progress {stage} failed: {operation}; rollback: {rollback}")]
    RemoteProgressApply {
        stage: &'static str,
        operation: String,
        rollback: String,
    },
    #[error("Local and Cloud progress disagree about Snapshot identity: {0}")]
    RemoteSnapshotIdentityConflict(String),
    #[error("Local Snapshot history changed while remote progress was being prepared")]
    LocalSnapshotHistoryChanged,
}

impl ServiceContext {
    pub async fn check_cloud_backend(
        &self,
        session: &CloudSyncSessionConfig,
    ) -> Result<CloudBackendCheckReport, BackendError> {
        Ok(session.check_report().await)
    }

    pub async fn upload_all_from_session(
        &self,
        session: &CloudSyncSessionConfig,
        token: Option<CancellationToken>,
    ) -> Result<BatchSyncReport, BackendError> {
        ensure_legacy_cloud_sync()?;
        upload_all_from_session(session, token).await
    }

    pub async fn download_all_from_session(
        &self,
        session: &CloudSyncSessionConfig,
        token: Option<CancellationToken>,
    ) -> Result<BatchSyncReport, BackendError> {
        ensure_legacy_cloud_sync()?;
        download_all_from_session(session, token).await
    }

    pub async fn sync_game(&self, game_name: &str) -> Result<SyncGameOutcome, BackendError> {
        ensure_legacy_cloud_sync()?;
        let config = get_config()?;
        let session = cloud_session(&config);
        let op = session.get_op()?;
        let game = find_game(&config, game_name)?;
        sync_cloud_game(&session, &op, &game).await
    }

    pub async fn resolve_game_conflict(
        &self,
        game_name: &str,
        resolution: ConflictResolution,
    ) -> Result<ConflictResolutionOutcome, BackendError> {
        ensure_legacy_cloud_sync()?;
        let config = get_config()?;
        let session = cloud_session(&config);
        let op = session.get_op()?;
        let game = find_game(&config, game_name)?;
        let outcome = resolve_cloud_conflict(&session, &op, &game, resolution).await?;

        if outcome == ConflictResolutionOutcome::AcceptedRemote {
            let snapshots = game.get_game_snapshots_info()?;
            self.pipeline()
                .fire_metadata_changed(&MetadataChangedCtx {
                    config,
                    source: HookSource::CloudSync,
                    game,
                    snapshots,
                })
                .await;
        }

        Ok(outcome)
    }

    pub async fn sync_config(&self) -> Result<(), BackendError> {
        ensure_legacy_cloud_sync()?;
        let config = get_config()?;
        let session = cloud_session(&config);
        let op = session.get_op()?;
        sync_cloud_config(&session, &op, &config).await
    }

    pub async fn inspect_cloud_library(
        &self,
    ) -> Result<CloudLibraryStatus, CloudLibraryServiceError> {
        let (_, _, local_state) = cloud_bootstrap_inputs()?;
        let generation = local_state.cloud_namespace_generation;
        let session = CloudSyncSessionConfig::from(&local_state.cloud_settings);
        let operator = session.get_op()?;
        let bootstrap = CloudLibraryBootstrap::new(operator.clone(), 3);
        let resumable_cutover = cutover_progress_path(&local_state.cloud_settings)?.is_file();
        match bootstrap.inspect().await {
            Ok(classification) => {
                if generation == CloudNamespaceGeneration::V2
                    && let CloudNamespaceClassification::SupportedV2 {
                        descriptor,
                        shared_library,
                        ..
                    } = &classification
                {
                    if local_state.cloud_library_id.as_deref()
                        != Some(descriptor.library_id.as_str())
                    {
                        return Ok(CloudLibraryStatus::ReconnectRequired {
                            game_count: shared_library.games.len(),
                        });
                    }
                    let profile_missing = !DeviceProfileRepository::new(operator, 3)
                        .list()
                        .await?
                        .iter()
                        .any(|profile| profile.device.id == local_state.current_device_id);
                    if profile_missing {
                        return Ok(CloudLibraryStatus::ReconnectRequired {
                            game_count: shared_library.games.len(),
                        });
                    }
                }
                map_library_status(generation, classification, resumable_cutover)
            }
            Err(error) => map_inspect_error(generation, resumable_cutover, error),
        }
    }

    /// Rebuild a missing or corrupt remote V2 namespace from this Device's
    /// accepted local owners and locally available enabled Snapshots.
    pub async fn rebuild_cloud_library_from_local(
        &self,
        confirmed: bool,
    ) -> Result<CloudLibraryStatus, CloudLibraryServiceError> {
        if !confirmed {
            return Err(CloudLibraryServiceError::ConfirmationRequired);
        }
        let (shared_library, device_profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let session = CloudSyncSessionConfig::from(&local_state.cloud_settings);
        let operator = session.get_op()?;
        let library_id = local_state
            .cloud_library_id
            .as_deref()
            .ok_or(CloudLibraryServiceError::ActiveLibraryUnavailable)?;
        match operator
            .delete_with(crate::cloud_sync::v2::V2_NAMESPACE_PREFIX)
            .recursive(true)
            .await
        {
            Ok(()) => {}
            Err(err) if err.kind() == opendal::ErrorKind::NotFound => {}
            Err(err) => return Err(BackendError::from(err).into()),
        }
        let descriptor = CloudNamespaceDescriptor::with_library_id(library_id);
        CloudLibraryBootstrap::new(operator, 3)
            .create_empty(&descriptor, &shared_library, &device_profile)
            .await?;
        self.reupload_enabled_local_progress(&CancellationToken::new())
            .await?;
        Ok(CloudLibraryStatus::Active {
            game_count: shared_library.games.len(),
        })
    }

    /// Re-register only this Device against the remote library. No remote
    /// namespace or Snapshot is deleted by this operation.
    pub async fn reconnect_cloud_library(
        &self,
        confirmed: bool,
    ) -> Result<CloudLibraryStatus, CloudLibraryServiceError> {
        if !confirmed {
            return Err(CloudLibraryServiceError::ConfirmationRequired);
        }
        let (expected_library, expected_profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let session = CloudSyncSessionConfig::from(&local_state.cloud_settings);
        let operator = session.get_op()?;
        let classification = CloudLibraryBootstrap::new(operator.clone(), 3)
            .inspect()
            .await?;
        let CloudNamespaceClassification::SupportedV2 {
            descriptor,
            shared_library,
            ..
        } = classification
        else {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        };
        let published = crate::config::connected_cloud_profile(
            &expected_library,
            &expected_profile,
            &local_state,
            &shared_library,
        )?;
        DeviceProfileRepository::new(operator, 3)
            .publish(&local_state.current_device_id, &published)
            .await?;
        crate::config::connect_cloud_library_local(
            &expected_library,
            &expected_profile,
            &local_state,
            &shared_library,
            &descriptor.library_id,
        )?;
        Ok(CloudLibraryStatus::Active {
            game_count: shared_library.games.len(),
        })
    }

    pub async fn create_cloud_library(
        &self,
        confirmed: bool,
    ) -> Result<CloudLibraryStatus, CloudLibraryServiceError> {
        if !confirmed {
            return Err(CloudLibraryServiceError::ConfirmationRequired);
        }
        let (shared_library, device_profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation == CloudNamespaceGeneration::V2 {
            return self.inspect_cloud_library().await;
        }

        let session = CloudSyncSessionConfig::from(&local_state.cloud_settings);
        let bootstrap = CloudLibraryBootstrap::new(session.get_op()?, 3);
        let descriptor = CloudNamespaceDescriptor::default();
        bootstrap
            .create_empty(&descriptor, &shared_library, &device_profile)
            .await?;
        activate_cloud_namespace_v2(&shared_library, &device_profile, &descriptor.library_id)?;
        self.publish_enabled_local_progress(&CancellationToken::new())
            .await?;
        Ok(CloudLibraryStatus::Active {
            game_count: shared_library.games.len(),
        })
    }

    pub async fn review_cloud_library_join(
        &self,
    ) -> Result<CloudLibraryJoinReview, CloudLibraryServiceError> {
        let (local_library, _, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation == CloudNamespaceGeneration::V2 {
            return self.review_pending_definitions().await;
        }
        let session = CloudSyncSessionConfig::from(&local_state.cloud_settings);
        Ok(CloudLibraryJoin::new(session.get_op()?, 3)
            .review(&local_library)
            .await?)
    }

    pub async fn join_cloud_library(
        &self,
        decisions: &[JoinGameDecision],
        confirmed_replacements: bool,
    ) -> Result<CloudLibraryJoinOutcome, CloudLibraryServiceError> {
        let (local_library, local_profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation == CloudNamespaceGeneration::V2 {
            return self
                .resolve_pending_definitions(decisions, confirmed_replacements)
                .await;
        }
        let session = CloudSyncSessionConfig::from(&local_state.cloud_settings);
        let joined = CloudLibraryJoin::new(session.get_op()?, 3)
            .join(
                &local_library,
                &local_profile,
                decisions,
                confirmed_replacements,
            )
            .await;
        let joined = match joined {
            Ok(joined) => joined,
            Err(CloudLibraryJoinError::TargetChanged(game_name))
            | Err(CloudLibraryJoinError::DecisionRequired(game_name))
            | Err(CloudLibraryJoinError::LocalGameChanged(game_name)) => {
                return Ok(CloudLibraryJoinOutcome::ReviewChanged { game_name });
            }
            Err(error) => return Err(error.into()),
        };
        activate_joined_cloud_library(
            &local_library,
            &local_profile,
            &joined.shared_library,
            &joined.device_profile,
            &joined.library_id,
        )?;
        self.publish_enabled_local_progress(&CancellationToken::new())
            .await?;
        Ok(CloudLibraryJoinOutcome::Active {
            game_count: joined.shared_library.games.len(),
        })
    }

    pub async fn review_cloud_library_cutover(
        &self,
    ) -> Result<CloudLibraryCutoverReview, CloudLibraryServiceError> {
        let (_, profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::LegacyV1 {
            return Err(CloudLibraryServiceError::CutoverNotRequired);
        }
        self.cutover(&profile, &local_state)?
            .review()
            .await
            .map_err(Into::into)
    }

    pub async fn cutover_cloud_library(
        &self,
        confirmed: bool,
    ) -> Result<CloudLibraryCutoverOutcome, CloudLibraryServiceError> {
        if !confirmed {
            return Err(CloudLibraryServiceError::ConfirmationRequired);
        }
        let (local_library, local_profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::LegacyV1 {
            return Err(CloudLibraryServiceError::CutoverNotRequired);
        }
        let cutover = self.cutover(&local_profile, &local_state)?;
        let result = cutover.execute().await?;
        activate_cutover_cloud_library(
            &local_library,
            &local_profile,
            &result.shared_library,
            &result.device_profiles,
            &result.library_id,
        )?;
        if let Err(error) = cutover.finish().await {
            log::warn!(
                target: "rgsm::cloud::cutover",
                "V2 activation succeeded but local Cutover cleanup failed: {error}"
            );
        }
        Ok(CloudLibraryCutoverOutcome {
            game_count: result.shared_library.games.len(),
            snapshot_count: result.snapshot_count,
            unavailable_archives: result.unavailable_archives,
        })
    }

    pub async fn review_v2_game_progress(
        &self,
        game_id: &str,
    ) -> Result<V2ConflictReview, CloudLibraryServiceError> {
        let (library, profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let game_name = library
            .games
            .iter()
            .find(|game| game.storage_key == game_id)
            .map(|game| game.name.clone())
            .ok_or_else(|| CloudLibraryServiceError::GameProfileNotFound(game_id.to_string()))?;
        let local = get_config()?
            .games
            .into_iter()
            .find(|game| game.storage_key == game_id)
            .map(|game| match game.get_game_snapshots_info() {
                Ok(snapshots) => Ok(snapshots),
                Err(crate::preclude::BackupError::Io(error))
                    if error.kind() == std::io::ErrorKind::NotFound =>
                {
                    Ok(GameSnapshots::new(game.name))
                }
                Err(error) => Err(error),
            })
            .transpose()?
            .unwrap_or_else(|| GameSnapshots::new(game_name));
        let local_archive_root = profile
            .local_archive_root
            .as_deref()
            .map(resolve_app_path)
            .ok_or(CloudLibraryServiceError::StorageLocationRequired)?;
        Ok(V2ConflictInspector::new(
            bound_v2_operator(&local_state).await?,
            local_archive_root,
            local_state.current_device_id,
            3,
        )
        .review(game_id, &local)
        .await?)
    }

    pub async fn preview_materialize_all(
        &self,
    ) -> Result<MaterializationPreview, CloudLibraryServiceError> {
        Ok(self.materializer().await?.preview_materialize_all().await?)
    }

    pub async fn upload_cloud_archive(
        &self,
        game_id: &str,
        snapshot_id: &str,
    ) -> Result<(), CloudLibraryServiceError> {
        self.require_shared_game(game_id)?;
        let (_, profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let game = get_config()?
            .games
            .into_iter()
            .find(|game| game.storage_key == game_id || game.name == game_id)
            .ok_or_else(|| CloudLibraryServiceError::GameProfileNotFound(game_id.to_string()))?;
        let snapshots = game.get_game_snapshots_info()?;
        let snapshot = snapshots
            .backups
            .iter()
            .find(|item| item.date == snapshot_id)
            .cloned()
            .ok_or_else(|| {
                CloudLibraryServiceError::GameProfileNotFound(snapshot_id.to_string())
            })?;
        let local_archive_root = profile
            .local_archive_root
            .as_deref()
            .map(resolve_app_path)
            .ok_or(CloudLibraryServiceError::StorageLocationRequired)?;
        let coordinator = SnapshotSyncCoordinator::new(
            bound_v2_operator(&local_state).await?,
            local_archive_root,
            local_state.current_device_id.clone(),
            resolve_app_path("GameSaveManager.cloud-v2-materialization.json"),
            3,
        )
        .excluding_games(local_state.local_game_ids());
        Ok(coordinator
            .upload_local_snapshot(game_id, &snapshot, &snapshots)
            .await?)
    }

    pub async fn download_cloud_archive(
        &self,
        game_id: &str,
        snapshot_id: &str,
    ) -> Result<(), CloudLibraryServiceError> {
        let lineage = self
            .converged_materializer()
            .await?
            .download(game_id, snapshot_id)
            .await?;
        import_downloaded_lineage(game_id, &lineage)?;
        Ok(())
    }

    pub async fn materialize_all_cloud_archives(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<MaterializationOutcome, CloudLibraryServiceError> {
        let materializer = self.converged_materializer().await?;
        let outcome = materializer.materialize_all(cancellation).await;
        import_local_verified_catalog(&materializer).await?;
        Ok(outcome?)
    }

    pub async fn delete_v2_snapshot(
        &self,
        game_id: &str,
        snapshot_id: &str,
        confirmed: bool,
        current_position: Option<CurrentPositionDecision>,
    ) -> Result<(), CloudLibraryServiceError> {
        let (_, _, local_state) = cloud_bootstrap_inputs()?;
        if local_state
            .local_games
            .iter()
            .any(|game| game.storage_key == game_id)
        {
            if !confirmed {
                return Err(CloudLibraryServiceError::ConfirmationRequired);
            }
            let game = get_config()?
                .games
                .into_iter()
                .find(|game| game.storage_key == game_id)
                .ok_or_else(|| {
                    CloudLibraryServiceError::GameProfileNotFound(game_id.to_string())
                })?;
            return Ok(self
                .delete_snapshot(&game, snapshot_id, HookSource::UserManual)
                .await?);
        }
        // Only execute the Current Position decision when the caller has
        // confirmed deletion. Unconfirmed requests must not have destructive
        // side effects (Apply restores files, Capture creates a snapshot).
        if confirmed {
            self.resolve_current_position_for_deletion(game_id, snapshot_id, current_position)
                .await?;
        }
        let materializer = self.converged_materializer().await?;
        let deletion = materializer
            .delete_snapshot(game_id, snapshot_id, confirmed)
            .await;
        if matches!(
            deletion,
            Err(MaterializationError::Deletion(
                SnapshotDeletionLifecycleError::SnapshotNotFound(_)
                    | SnapshotDeletionLifecycleError::GameNotFound(_)
            ))
        ) {
            if !confirmed {
                return Err(CloudLibraryServiceError::ConfirmationRequired);
            }
            let game = get_config()?
                .games
                .into_iter()
                .find(|game| game.storage_key == game_id)
                .ok_or_else(|| {
                    CloudLibraryServiceError::GameProfileNotFound(game_id.to_string())
                })?;
            return Ok(self
                .delete_snapshot(&game, snapshot_id, HookSource::UserManual)
                .await?);
        }
        let convergence = self.converge_local_tombstone_metadata(&materializer).await;
        deletion?;
        convergence
    }

    pub async fn set_game_sync_mode(
        &self,
        game_id: &str,
        mode: SyncMode,
        initial_catch_up: InitialCatchUpPolicy,
        live_save: Option<LiveSaveSyncOptions>,
        cancellation: &CancellationToken,
    ) -> Result<GameSyncModeOutcome, CloudLibraryServiceError> {
        self.set_game_cloud_policy(
            game_id,
            true,
            mode,
            initial_catch_up,
            live_save,
            cancellation,
        )
        .await
    }

    pub async fn set_game_cloud_policy(
        &self,
        game_id: &str,
        enabled: bool,
        mode: SyncMode,
        initial_catch_up: InitialCatchUpPolicy,
        live_save: Option<LiveSaveSyncOptions>,
        cancellation: &CancellationToken,
    ) -> Result<GameSyncModeOutcome, CloudLibraryServiceError> {
        self.require_shared_game(game_id)?;
        if let Some(options) = &live_save
            && options.process_name.trim().is_empty()
        {
            return Err(CloudLibraryServiceError::LiveSaveProcessRequired);
        }
        let (_, expected_profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let materializer = self.converged_materializer().await?;
        let mut accepted_profile = expected_profile.clone();
        let effective_config = get_config()?;
        let settings = accepted_profile
            .games
            .get_mut(game_id)
            .ok_or_else(|| CloudLibraryServiceError::GameProfileNotFound(game_id.to_string()))?;

        let was_enabled = settings.cloud_sync_enabled;
        let newly_enabled = enabled && !was_enabled;
        let activation_revision = if newly_enabled {
            Some(materializer.catalog_revision().await?)
        } else {
            settings.snapshot_sync_activation_revision
        };
        let local_baseline = if newly_enabled {
            effective_config
                .games
                .iter()
                .find(|game| game.storage_key == game_id)
                .ok_or_else(|| CloudLibraryServiceError::GameProfileNotFound(game_id.to_string()))?
                .get_game_snapshots_info()?
                .backups
                .into_iter()
                .map(|snapshot| snapshot.date)
                .collect()
        } else {
            settings.snapshot_sync_local_baseline.clone()
        };
        settings.cloud_sync_enabled = enabled;
        settings.sync_mode = mode;
        if newly_enabled {
            settings.snapshot_sync_activation_revision = activation_revision;
            settings.snapshot_sync_local_baseline = local_baseline.clone();
            settings.initial_catch_up = initial_catch_up;
            settings.multi_device_sync_suspended = false;
        }
        if let Some(options) = live_save {
            settings.live_save_process_name = Some(options.process_name.trim().to_string());
            settings.live_save_snapshot_on_exit = options.snapshot_on_exit;
        }

        let operator = bound_v2_operator(&local_state).await?;

        // When disabling cloud sync, clear this Device's advertised head
        // BEFORE publishing the disabled profile. If we published first and
        // the manifest mutation failed, a repeated disable would skip the
        // clear (was_enabled == false), leaving the stale head visible to
        // other Devices indefinitely.
        if !enabled && was_enabled {
            let device_id = local_state.current_device_id.clone();
            CloudManifestRepository::new(operator.clone(), CLOUD_MANIFEST_PATH, 3)
                .mutate(move |manifest| {
                    if let Some(game) = manifest.games.get_mut(game_id) {
                        game.clear_head(&device_id);
                    }
                    Ok(())
                })
                .await?;
        }

        DeviceProfileRepository::new(operator.clone(), 3)
            .publish(
                &local_state.current_device_id,
                &accepted_profile.without_local_games(&local_state),
            )
            .await?;
        replace_current_device_profile(&expected_profile, &accepted_profile)?;

        let published = if newly_enabled {
            let game = effective_config
                .games
                .iter()
                .find(|game| game.storage_key == game_id)
                .ok_or_else(|| {
                    CloudLibraryServiceError::GameProfileNotFound(game_id.to_string())
                })?;
            let snapshots = game.get_game_snapshots_info()?;
            let coordinator = SnapshotSyncCoordinator::new(
                operator,
                accepted_profile
                    .local_archive_root
                    .as_deref()
                    .map(resolve_app_path)
                    .ok_or(CloudLibraryServiceError::StorageLocationRequired)?,
                local_state.current_device_id.clone(),
                resolve_app_path("GameSaveManager.cloud-v2-materialization.json"),
                3,
            )
            .excluding_games(local_state.local_game_ids());
            coordinator
                .publish_local_game(
                    game_id,
                    &snapshots,
                    activation_revision.expect("re-enable records an activation revision"),
                    &local_baseline,
                    cancellation,
                )
                .await?
                .published
        } else {
            0
        };

        let downloaded = if enabled && initial_catch_up == InitialCatchUpPolicy::DownloadExisting {
            let revision = materializer.catalog_revision().await?;
            let outcome = materializer
                .materialize_game(game_id, revision, cancellation)
                .await;
            import_local_verified_catalog(&materializer).await?;
            outcome?.downloaded
        } else {
            0
        };
        Ok(GameSyncModeOutcome {
            mode,
            cloud_sync_enabled: enabled,
            published,
            downloaded,
        })
    }

    fn cutover(
        &self,
        profile: &crate::config::DeviceProfile,
        local_state: &crate::config::LocalState,
    ) -> Result<CloudLibraryCutover, CloudLibraryServiceError> {
        let archive_root =
            resolve_backup_path(profile.local_archive_root.as_deref().unwrap_or("save_data"));
        Ok(CloudLibraryCutover::new(
            CloudSyncSessionConfig::from(&local_state.cloud_settings).get_op()?,
            archive_root,
            cutover_progress_path(&local_state.cloud_settings)?,
            local_state.current_device_id.clone(),
            profile.clone(),
            3,
        ))
    }

    async fn publish_enabled_local_progress(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<(), CloudLibraryServiceError> {
        let config = get_config()?;
        let (_, profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Ok(());
        }
        let Some(local_archive_root) = profile.local_archive_root.as_deref().map(resolve_app_path)
        else {
            return Ok(());
        };
        let coordinator = SnapshotSyncCoordinator::new(
            bound_v2_operator(&local_state).await?,
            local_archive_root,
            local_state.current_device_id.clone(),
            resolve_app_path("GameSaveManager.cloud-v2-materialization.json"),
            3,
        )
        .excluding_games(local_state.local_game_ids());
        for (game_id, settings) in &profile.games {
            if !settings.cloud_sync_enabled || local_state.is_local_game(game_id) {
                continue;
            }
            let Some(game) = config
                .games
                .iter()
                .find(|game| game.storage_key == *game_id)
            else {
                continue;
            };
            let snapshots = match game.get_game_snapshots_info() {
                Ok(snapshots) => snapshots,
                Err(crate::preclude::BackupError::Io(error))
                    if error.kind() == std::io::ErrorKind::NotFound =>
                {
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            coordinator
                .publish_local_game(
                    game_id,
                    &snapshots,
                    settings.snapshot_sync_activation_revision.unwrap_or(0),
                    &BTreeSet::new(),
                    cancellation,
                )
                .await?;
        }
        Ok(())
    }

    async fn reupload_enabled_local_progress(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<(), CloudLibraryServiceError> {
        let config = get_config()?;
        let (_, profile, local_state) = cloud_bootstrap_inputs()?;
        let local_archive_root = profile
            .local_archive_root
            .as_deref()
            .map(resolve_app_path)
            .ok_or(CloudLibraryServiceError::StorageLocationRequired)?;
        let coordinator = SnapshotSyncCoordinator::new(
            bound_v2_operator(&local_state).await?,
            local_archive_root,
            local_state.current_device_id.clone(),
            resolve_app_path("GameSaveManager.cloud-v2-materialization.json"),
            3,
        )
        .excluding_games(local_state.local_game_ids());
        for (game_id, settings) in &profile.games {
            if !settings.cloud_sync_enabled || local_state.is_local_game(game_id) {
                continue;
            }
            let Some(game) = config
                .games
                .iter()
                .find(|game| game.storage_key == *game_id)
            else {
                continue;
            };
            coordinator
                .reconcile_game_with_policy(
                    game_id,
                    &game.get_game_snapshots_info()?,
                    settings.snapshot_sync_activation_revision.unwrap_or(0),
                    &BTreeSet::new(),
                    cancellation,
                    SnapshotReconcilePolicy {
                        upload_new_archives: true,
                        download_forward_target: false,
                    },
                )
                .await?;
        }
        Ok(())
    }

    async fn resolve_current_position_for_deletion(
        &self,
        game_id: &str,
        snapshot_id: &str,
        decision: Option<CurrentPositionDecision>,
    ) -> Result<(), CloudLibraryServiceError> {
        let config = get_config()?;
        let Some(game) = config.games.iter().find(|game| game.storage_key == game_id) else {
            return Ok(());
        };
        let snapshots = match game.get_game_snapshots_info() {
            Ok(snapshots) => snapshots,
            Err(crate::preclude::BackupError::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        if snapshots.current_device_head().map(String::as_str) != Some(snapshot_id) {
            return Ok(());
        }
        match decision {
            None => {
                return Err(CloudLibraryServiceError::CurrentPositionBlocksDeletion(
                    snapshot_id.to_string(),
                ));
            }
            Some(CurrentPositionDecision::Apply { snapshot_id: next }) => {
                if next == snapshot_id {
                    return Err(CloudLibraryServiceError::CurrentPositionBlocksDeletion(
                        snapshot_id.to_string(),
                    ));
                }
                self.restore_snapshot(game, &next, HookSource::UserManual, None)
                    .await?;
            }
            Some(CurrentPositionDecision::Capture) => {
                self.create_snapshot(game, "", HookSource::UserManual, None)
                    .await?;
            }
            Some(CurrentPositionDecision::Clear) => {
                let mut next = snapshots;
                next.set_current_device_head(None);
                game.set_game_snapshots_info(&next)?;
                self.publish_current_position(game_id, &next).await?;
            }
            Some(CurrentPositionDecision::FallbackToParent) => {
                // Walk past tombstoned/absent ancestors to find the nearest
                // live snapshot, or clear the position if none remain.
                let live_dates: BTreeSet<&str> = snapshots
                    .backups
                    .iter()
                    .map(|snapshot| snapshot.date.as_str())
                    .collect();
                let mut parent = snapshots
                    .backups
                    .iter()
                    .find(|snapshot| snapshot.date == snapshot_id)
                    .and_then(|snapshot| snapshot.parent.clone());
                while let Some(candidate) = &parent {
                    if live_dates.contains(candidate.as_str()) {
                        break;
                    }
                    // This ancestor is tombstoned/absent; walk further up.
                    let grandparent = snapshots
                        .backups
                        .iter()
                        .find(|snapshot| snapshot.date == *candidate)
                        .and_then(|snapshot| snapshot.parent.clone());
                    parent = grandparent;
                }
                let parent = parent.filter(|candidate| live_dates.contains(candidate.as_str()));
                let mut next = snapshots;
                next.set_current_device_head(parent);
                game.set_game_snapshots_info(&next)?;
                self.publish_current_position(game_id, &next).await?;
            }
        }
        Ok(())
    }

    async fn publish_current_position(
        &self,
        game_id: &str,
        local: &GameSnapshots,
    ) -> Result<(), CloudLibraryServiceError> {
        let (_, profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Ok(());
        }
        let Some(local_archive_root) = profile.local_archive_root.as_deref().map(resolve_app_path)
        else {
            return Ok(());
        };
        SnapshotSyncCoordinator::new(
            bound_v2_operator(&local_state).await?,
            local_archive_root,
            local_state.current_device_id,
            resolve_app_path("GameSaveManager.cloud-v2-materialization.json"),
            3,
        )
        .publish_current_head(game_id, local)
        .await?;
        Ok(())
    }

    pub(super) async fn set_multi_device_sync_suspended(
        &self,
        game_id: &str,
        suspended: bool,
    ) -> Result<(), CloudLibraryServiceError> {
        set_multi_device_sync_suspended(game_id, suspended).await
    }
}

pub(super) async fn set_multi_device_sync_suspended(
    game_id: &str,
    suspended: bool,
) -> Result<(), CloudLibraryServiceError> {
    let (_, expected, local_state) = cloud_bootstrap_inputs()?;
    if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
        return Ok(());
    }
    let mut accepted = expected.clone();
    let Some(settings) = accepted.games.get_mut(game_id) else {
        return Ok(());
    };
    if settings.multi_device_sync_suspended == suspended {
        return Ok(());
    }
    settings.multi_device_sync_suspended = suspended;
    DeviceProfileRepository::new(bound_v2_operator(&local_state).await?, 3)
        .publish(
            &local_state.current_device_id,
            &accepted.without_local_games(&local_state),
        )
        .await?;
    replace_current_device_profile(&expected, &accepted)?;
    Ok(())
}

fn cutover_progress_path(
    cloud_settings: &crate::cloud_sync::CloudSettings,
) -> Result<std::path::PathBuf, CloudLibraryServiceError> {
    Ok(crate::app_dirs::get_app_data_dir().join(cutover_progress_file_name(cloud_settings)?))
}

fn cutover_progress_file_name(
    cloud_settings: &crate::cloud_sync::CloudSettings,
) -> Result<String, CloudLibraryServiceError> {
    let identity = xxhash_rust::xxh3::xxh3_64(&serde_json::to_vec(cloud_settings)?);
    Ok(format!(
        "GameSaveManager.cloud-cutover.{identity:016x}.json"
    ))
}

fn map_library_status(
    generation: CloudNamespaceGeneration,
    classification: CloudNamespaceClassification,
    resumable_cutover: bool,
) -> Result<CloudLibraryStatus, CloudLibraryServiceError> {
    match (generation, classification) {
        (
            CloudNamespaceGeneration::V2,
            CloudNamespaceClassification::SupportedV2 { shared_library, .. },
        ) => Ok(CloudLibraryStatus::Active {
            game_count: shared_library.games.len(),
        }),
        (CloudNamespaceGeneration::V2, CloudNamespaceClassification::Empty) => {
            Ok(CloudLibraryStatus::RebuildRequired)
        }
        (CloudNamespaceGeneration::V2, _) => {
            Err(CloudLibraryServiceError::ActiveLibraryUnavailable)
        }
        (
            CloudNamespaceGeneration::LegacyV1,
            CloudNamespaceClassification::SupportedV2 { shared_library, .. },
        ) => Ok(CloudLibraryStatus::JoinRequired {
            game_count: shared_library.games.len(),
        }),
        (CloudNamespaceGeneration::LegacyV1, CloudNamespaceClassification::V1Only { config }) => {
            Ok(CloudLibraryStatus::CutoverRequired {
                game_count: config.games.len(),
                resumable: resumable_cutover,
            })
        }
        (CloudNamespaceGeneration::LegacyV1, CloudNamespaceClassification::Empty) => {
            Ok(CloudLibraryStatus::Empty)
        }
    }
}

fn map_inspect_error(
    generation: CloudNamespaceGeneration,
    resumable_cutover: bool,
    error: CloudLibraryBootstrapError,
) -> Result<CloudLibraryStatus, CloudLibraryServiceError> {
    match error {
        CloudLibraryBootstrapError::Namespace(
            CloudNamespaceError::MalformedObject { .. }
            | CloudNamespaceError::MissingRequiredObject(_)
            | CloudNamespaceError::PartialV2(_)
            | CloudNamespaceError::SharedLibrary(_)
            | CloudNamespaceError::Manifest(_),
        ) if generation == CloudNamespaceGeneration::V2 => Ok(CloudLibraryStatus::RebuildRequired),
        CloudLibraryBootstrapError::Namespace(CloudNamespaceError::PartialV2(_))
            if generation == CloudNamespaceGeneration::LegacyV1 && resumable_cutover =>
        {
            Ok(CloudLibraryStatus::CutoverRequired {
                game_count: 0,
                resumable: true,
            })
        }
        error => Err(error.into()),
    }
}

fn import_downloaded_lineage(
    game_id: &str,
    lineage: &[crate::backup::Snapshot],
) -> Result<(), CloudLibraryServiceError> {
    if lineage.is_empty() {
        return Ok(());
    }
    let game = get_config()?
        .games
        .into_iter()
        .find(|game| game.storage_key == game_id || game.name == game_id)
        .ok_or_else(|| CloudLibraryServiceError::GameProfileNotFound(game_id.to_string()))?;
    let mut local = match game.get_game_snapshots_info() {
        Ok(snapshots) => snapshots,
        Err(crate::preclude::BackupError::Io(error))
            if error.kind() == std::io::ErrorKind::NotFound =>
        {
            GameSnapshots::new(game.name.clone())
        }
        Err(error) => return Err(error.into()),
    };
    super::conflict_resolution::merge_remote_lineage(&mut local, lineage)?;
    game.set_game_snapshots_info(&local)?;
    Ok(())
}

pub(crate) async fn import_local_verified_catalog(
    materializer: &CloudArchiveMaterializer,
) -> Result<(), CloudLibraryServiceError> {
    let catalog = materializer.imported_local_catalog().await?;
    for (game_id, lineage) in catalog {
        import_downloaded_lineage(&game_id, &lineage)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::cloud_sync::v2::{CloudManifest, CloudNamespaceDescriptor, device_profile_path};
    use crate::cloud_sync::{Backend, CloudSettings};
    use crate::config::{
        Config, ConfigTestStateGuard, SharedLibrary, V2_CONFIG_SCHEMA_VERSION, set_config_local,
    };
    use crate::hooks::HookPipeline;

    const LOCAL_LIBRARY_ID: &str = "11111111-1111-4111-8111-111111111111";
    const REMOTE_LIBRARY_ID: &str = "22222222-2222-4222-8222-222222222222";

    fn supported() -> CloudNamespaceClassification {
        CloudNamespaceClassification::SupportedV2 {
            descriptor: CloudNamespaceDescriptor::default(),
            shared_library: SharedLibrary {
                schema_version: V2_CONFIG_SCHEMA_VERSION,
                games: Vec::new(),
            },
            manifest: CloudManifest::default(),
        }
    }

    fn v1_only() -> CloudNamespaceClassification {
        CloudNamespaceClassification::V1Only {
            config: Box::new(Config::default()),
        }
    }

    #[test]
    fn remote_state_maps_to_explicit_player_actions() {
        assert_eq!(
            map_library_status(CloudNamespaceGeneration::LegacyV1, supported(), false).unwrap(),
            CloudLibraryStatus::JoinRequired { game_count: 0 }
        );
        assert_eq!(
            map_library_status(
                CloudNamespaceGeneration::LegacyV1,
                CloudNamespaceClassification::Empty,
                false
            )
            .unwrap(),
            CloudLibraryStatus::Empty
        );
        assert_eq!(
            map_library_status(CloudNamespaceGeneration::V2, supported(), false).unwrap(),
            CloudLibraryStatus::Active { game_count: 0 }
        );
        assert_eq!(
            map_library_status(
                CloudNamespaceGeneration::V2,
                CloudNamespaceClassification::Empty,
                false
            )
            .unwrap(),
            CloudLibraryStatus::RebuildRequired
        );
        assert_eq!(
            map_library_status(CloudNamespaceGeneration::LegacyV1, v1_only(), false).unwrap(),
            CloudLibraryStatus::CutoverRequired {
                game_count: 0,
                resumable: false,
            }
        );
        assert_eq!(
            map_library_status(CloudNamespaceGeneration::LegacyV1, v1_only(), true).unwrap(),
            CloudLibraryStatus::CutoverRequired {
                game_count: 0,
                resumable: true,
            }
        );
    }

    #[test]
    fn active_device_fails_closed_when_saved_location_contains_legacy_data() {
        assert!(matches!(
            map_library_status(CloudNamespaceGeneration::V2, v1_only(), false),
            Err(CloudLibraryServiceError::ActiveLibraryUnavailable)
        ));
    }

    #[test]
    fn identity_mismatch_bypasses_unrelated_profile_corruption() {
        let _config_lock = crate::config::lock_config_test_file();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let cloud_root = temp_dir::TempDir::new().unwrap();
            let mut config = Config::default();
            config.settings.cloud_settings = CloudSettings {
                backend: Backend::Fs,
                root_path: cloud_root.path().to_string_lossy().into_owned(),
                ..CloudSettings::default()
            };
            let _config_state = ConfigTestStateGuard::replace_with(&config).unwrap();
            set_config_local(&config).unwrap();
            let (library, profile, _) = cloud_bootstrap_inputs().unwrap();
            activate_cloud_namespace_v2(&library, &profile, LOCAL_LIBRARY_ID).unwrap();

            let operator = CloudSyncSessionConfig::from(&config.settings.cloud_settings)
                .get_op()
                .unwrap();
            CloudLibraryBootstrap::new(operator.clone(), 2)
                .create_empty(
                    &CloudNamespaceDescriptor::with_library_id(REMOTE_LIBRARY_ID),
                    &library,
                    &profile,
                )
                .await
                .unwrap();
            operator
                .write(&device_profile_path("broken-device"), b"{".to_vec())
                .await
                .unwrap();

            let service = ServiceContext::new(Arc::new(HookPipeline::new(vec![])));
            assert_eq!(
                service.inspect_cloud_library().await.unwrap(),
                CloudLibraryStatus::ReconnectRequired {
                    game_count: library.games.len(),
                }
            );
        });
    }

    #[test]
    fn active_device_can_rebuild_a_namespace_with_missing_v2_objects() {
        assert_eq!(
            map_inspect_error(
                CloudNamespaceGeneration::V2,
                false,
                CloudLibraryBootstrapError::Namespace(CloudNamespaceError::MissingRequiredObject(
                    "v2/shared-library.json"
                )),
            )
            .unwrap(),
            CloudLibraryStatus::RebuildRequired
        );
    }

    fn resumable_partial_v2_status(
        generation: CloudNamespaceGeneration,
        resumable_cutover: bool,
    ) -> Result<CloudLibraryStatus, CloudLibraryServiceError> {
        map_inspect_error(
            generation,
            resumable_cutover,
            CloudLibraryBootstrapError::Namespace(CloudNamespaceError::PartialV2(vec![
                "v2/archives/".into(),
            ])),
        )
    }

    #[test]
    fn interrupted_cutover_with_local_progress_is_resumable() {
        assert_eq!(
            resumable_partial_v2_status(CloudNamespaceGeneration::LegacyV1, true).unwrap(),
            CloudLibraryStatus::CutoverRequired {
                game_count: 0,
                resumable: true,
            }
        );
    }

    #[test]
    fn partial_v2_without_local_progress_requires_the_matching_recovery_path() {
        assert!(matches!(
            resumable_partial_v2_status(CloudNamespaceGeneration::LegacyV1, false),
            Err(CloudLibraryServiceError::Bootstrap(
                CloudLibraryBootstrapError::Namespace(CloudNamespaceError::PartialV2(_))
            ))
        ));
        assert_eq!(
            resumable_partial_v2_status(CloudNamespaceGeneration::V2, true).unwrap(),
            CloudLibraryStatus::RebuildRequired
        );
    }

    #[test]
    fn cutover_progress_file_follows_saved_cloud_settings() {
        let mut settings = CloudSettings::default();
        let first = cutover_progress_file_name(&settings).unwrap();
        assert_eq!(first, cutover_progress_file_name(&settings).unwrap());
        settings.root_path = "/other-root".into();
        assert_ne!(first, cutover_progress_file_name(&settings).unwrap());
    }
}
