use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::app_dirs::resolve_app_path;
use crate::backup::{Game, GameSnapshots};
use crate::cloud_sync::v2::{
    AcceptRemoteProgressError, CloudArchiveMaterializer, CloudLibraryBootstrap,
    CloudLibraryBootstrapError, CloudLibraryCutover, CloudLibraryCutoverError,
    CloudLibraryCutoverReview, CloudLibraryJoin, CloudLibraryJoinError, CloudLibraryJoinReview,
    CloudNamespaceClassification, ConflictReviewError, DeviceProfileRepository,
    DeviceProfileRepositoryError, JoinGameDecision, KeepLocalProgressError,
    LocalArchiveEvictionError, ManifestRepositoryError, MaterializationError,
    MaterializationOutcome, MaterializationPreview, SharedLibraryRepositoryError,
    SnapshotSyncError, V2ConflictInspector, V2ConflictReview,
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

use super::ServiceContext;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CloudLibraryStatus {
    Empty,
    JoinRequired { game_count: usize },
    CutoverRequired { game_count: usize },
    Active { game_count: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CloudLibraryJoinOutcome {
    Active { game_count: usize },
    ReviewChanged { game_name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CloudLibraryCutoverOutcome {
    pub game_count: usize,
    pub snapshot_count: usize,
    pub unavailable_archives: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct GameSyncModeOutcome {
    pub mode: SyncMode,
    pub downloaded: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct LiveSaveSyncOptions {
    pub process_name: String,
    pub snapshot_on_exit: bool,
}

#[derive(Debug, Error)]
pub enum CloudLibraryServiceError {
    #[error("Cloud Library creation requires explicit confirmation")]
    ConfirmationRequired,
    #[error("The active V2 Cloud Library no longer matches the saved connection")]
    ActiveLibraryUnavailable,
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
        let bootstrap = CloudLibraryBootstrap::new(session.get_op()?, 3);
        let classification = bootstrap.inspect().await?;
        map_library_status(generation, classification)
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
        bootstrap
            .create_empty(&shared_library, &device_profile)
            .await?;
        activate_cloud_namespace_v2(&shared_library, &device_profile)?;
        Ok(CloudLibraryStatus::Active {
            game_count: shared_library.games.len(),
        })
    }

    pub async fn review_cloud_library_join(
        &self,
    ) -> Result<CloudLibraryJoinReview, CloudLibraryServiceError> {
        let (local_library, _, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::LegacyV1 {
            return Err(CloudLibraryServiceError::JoinNotRequired);
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
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::LegacyV1 {
            return Err(CloudLibraryServiceError::JoinNotRequired);
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
        let (accepted_library, accepted_profile) = match joined {
            Ok(joined) => joined,
            Err(CloudLibraryJoinError::TargetChanged(game_name))
            | Err(CloudLibraryJoinError::LocalGameChanged(game_name)) => {
                return Ok(CloudLibraryJoinOutcome::ReviewChanged { game_name });
            }
            Err(error) => return Err(error.into()),
        };
        activate_joined_cloud_library(
            &local_library,
            &local_profile,
            &accepted_library,
            &accepted_profile,
        )?;
        Ok(CloudLibraryJoinOutcome::Active {
            game_count: accepted_library.games.len(),
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
            .map(|game| game.get_game_snapshots_info())
            .transpose()?
            .unwrap_or_else(|| GameSnapshots::new(game_name));
        let local_archive_root = profile
            .local_archive_root
            .as_deref()
            .map(resolve_app_path)
            .ok_or(CloudLibraryServiceError::StorageLocationRequired)?;
        let session = CloudSyncSessionConfig::from(&local_state.cloud_settings);
        Ok(V2ConflictInspector::new(
            session.get_op()?,
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
        Ok(self
            .converged_materializer()
            .await?
            .preview_materialize_all()
            .await?)
    }

    pub async fn upload_cloud_archive(
        &self,
        game_id: &str,
        snapshot_id: &str,
    ) -> Result<(), CloudLibraryServiceError> {
        Ok(self
            .converged_materializer()
            .await?
            .upload(game_id, snapshot_id)
            .await?)
    }

    pub async fn download_cloud_archive(
        &self,
        game_id: &str,
        snapshot_id: &str,
    ) -> Result<(), CloudLibraryServiceError> {
        Ok(self
            .converged_materializer()
            .await?
            .download(game_id, snapshot_id)
            .await?)
    }

    pub async fn materialize_all_cloud_archives(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<MaterializationOutcome, CloudLibraryServiceError> {
        Ok(self
            .converged_materializer()
            .await?
            .materialize_all(cancellation)
            .await?)
    }

    pub async fn delete_v2_snapshot(
        &self,
        game_id: &str,
        snapshot_id: &str,
        confirmed: bool,
    ) -> Result<(), CloudLibraryServiceError> {
        let materializer = self.converged_materializer().await?;
        let deletion = materializer
            .delete_snapshot(game_id, snapshot_id, confirmed)
            .await;
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
        let live_save = if mode == SyncMode::LiveSaveSync {
            let options = live_save.ok_or(CloudLibraryServiceError::LiveSaveProcessRequired)?;
            let process_name = options.process_name.trim();
            if process_name.is_empty() {
                return Err(CloudLibraryServiceError::LiveSaveProcessRequired);
            }
            Some(LiveSaveSyncOptions {
                process_name: process_name.to_string(),
                snapshot_on_exit: options.snapshot_on_exit,
            })
        } else {
            None
        };
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

        let synchronized = mode != SyncMode::Manual;
        let newly_enabled = synchronized
            && (settings.sync_mode == SyncMode::Manual
                || settings.snapshot_sync_activation_revision.is_none());
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
        settings.sync_mode = mode;
        settings.snapshot_sync_activation_revision = if synchronized {
            activation_revision
        } else {
            None
        };
        settings.snapshot_sync_local_baseline = if synchronized {
            local_baseline
        } else {
            Default::default()
        };
        settings.initial_catch_up = if synchronized {
            initial_catch_up
        } else {
            InitialCatchUpPolicy::KeepRemote
        };
        if let Some(options) = live_save {
            settings.live_save_process_name = Some(options.process_name);
            settings.live_save_snapshot_on_exit = options.snapshot_on_exit;
        }

        let session = CloudSyncSessionConfig::from(&local_state.cloud_settings);
        DeviceProfileRepository::new(session.get_op()?, 3)
            .publish(&local_state.current_device_id, &accepted_profile)
            .await?;
        replace_current_device_profile(&expected_profile, &accepted_profile)?;

        let downloaded = if newly_enabled
            && initial_catch_up == InitialCatchUpPolicy::DownloadExisting
        {
            materializer
                .materialize_game(
                    game_id,
                    activation_revision.expect("new synchronized mode has an activation revision"),
                    cancellation,
                )
                .await?
                .downloaded
        } else {
            0
        };
        Ok(GameSyncModeOutcome { mode, downloaded })
    }

    fn cutover(
        &self,
        profile: &crate::config::DeviceProfile,
        local_state: &crate::config::LocalState,
    ) -> Result<CloudLibraryCutover, CloudLibraryServiceError> {
        let session = CloudSyncSessionConfig::from(&local_state.cloud_settings);
        let identity =
            xxhash_rust::xxh3::xxh3_64(&serde_json::to_vec(&local_state.cloud_settings)?);
        let progress_path = crate::app_dirs::get_app_data_dir().join(format!(
            "GameSaveManager.cloud-cutover.{identity:016x}.json"
        ));
        let archive_root =
            resolve_backup_path(profile.local_archive_root.as_deref().unwrap_or("save_data"));
        Ok(CloudLibraryCutover::new(
            session.get_op()?,
            archive_root,
            progress_path,
            local_state.current_device_id.clone(),
            3,
        ))
    }

    fn materializer(&self) -> Result<CloudArchiveMaterializer, CloudLibraryServiceError> {
        let (_, profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let local_archive_root = profile
            .local_archive_root
            .as_deref()
            .map(resolve_app_path)
            .ok_or(CloudLibraryServiceError::StorageLocationRequired)?;
        let session = CloudSyncSessionConfig::from(&local_state.cloud_settings);
        Ok(CloudArchiveMaterializer::new(
            session.get_op()?,
            local_archive_root,
            local_state.current_device_id,
            resolve_app_path("GameSaveManager.cloud-v2-materialization.json"),
            3,
        ))
    }

    pub(super) async fn converged_materializer(
        &self,
    ) -> Result<CloudArchiveMaterializer, CloudLibraryServiceError> {
        let materializer = self.materializer()?;
        self.converge_local_tombstone_metadata(&materializer)
            .await?;
        Ok(materializer)
    }

    async fn converge_local_tombstone_metadata(
        &self,
        materializer: &CloudArchiveMaterializer,
    ) -> Result<(), CloudLibraryServiceError> {
        let tombstones = materializer.converge_local_tombstones().await?;
        if tombstones.is_empty() {
            return Ok(());
        }
        let config = get_config()?;
        for game in &config.games {
            if let Some(snapshot_ids) = tombstones.get(&game.storage_key) {
                game.forget_v2_tombstones(snapshot_ids)?;
            }
        }
        Ok(())
    }
}

fn map_library_status(
    generation: CloudNamespaceGeneration,
    classification: CloudNamespaceClassification,
) -> Result<CloudLibraryStatus, CloudLibraryServiceError> {
    match (generation, classification) {
        (
            CloudNamespaceGeneration::V2,
            CloudNamespaceClassification::SupportedV2 { shared_library, .. },
        ) => Ok(CloudLibraryStatus::Active {
            game_count: shared_library.games.len(),
        }),
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
            })
        }
        (CloudNamespaceGeneration::LegacyV1, CloudNamespaceClassification::Empty) => {
            Ok(CloudLibraryStatus::Empty)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud_sync::v2::{CloudManifest, CloudNamespaceDescriptor};
    use crate::config::{SharedLibrary, V2_CONFIG_SCHEMA_VERSION};

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

    #[test]
    fn remote_state_maps_to_explicit_player_actions() {
        assert_eq!(
            map_library_status(CloudNamespaceGeneration::LegacyV1, supported()).unwrap(),
            CloudLibraryStatus::JoinRequired { game_count: 0 }
        );
        assert_eq!(
            map_library_status(
                CloudNamespaceGeneration::LegacyV1,
                CloudNamespaceClassification::Empty
            )
            .unwrap(),
            CloudLibraryStatus::Empty
        );
        assert_eq!(
            map_library_status(CloudNamespaceGeneration::V2, supported()).unwrap(),
            CloudLibraryStatus::Active { game_count: 0 }
        );
    }

    #[test]
    fn active_device_fails_closed_when_saved_location_is_not_v2() {
        assert!(matches!(
            map_library_status(
                CloudNamespaceGeneration::V2,
                CloudNamespaceClassification::Empty
            ),
            Err(CloudLibraryServiceError::ActiveLibraryUnavailable)
        ));
    }
}
