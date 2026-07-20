use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::backup::Game;
use crate::cloud_sync::v2::{
    CloudLibraryBootstrap, CloudLibraryBootstrapError, CloudLibraryJoin, CloudLibraryJoinError,
    CloudLibraryJoinReview, CloudNamespaceClassification, JoinGameDecision,
};
use crate::cloud_sync::{
    BatchSyncReport, CloudBackendCheckReport, CloudSyncSessionConfig, ConflictResolution,
    ConflictResolutionOutcome, SyncGameOutcome, download_all_from_session,
    resolve_game_conflict as resolve_cloud_conflict, sync_config as sync_cloud_config,
    sync_game as sync_cloud_game, upload_all_from_session,
};
use crate::config::{
    CloudNamespaceGeneration, Config, activate_cloud_namespace_v2, activate_joined_cloud_library,
    cloud_bootstrap_inputs, cloud_namespace_generation, get_config,
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

#[derive(Debug, Error)]
pub enum CloudLibraryServiceError {
    #[error("Cloud Library creation requires explicit confirmation")]
    ConfirmationRequired,
    #[error("The active V2 Cloud Library no longer matches the saved connection")]
    ActiveLibraryUnavailable,
    #[error("This installation does not need to join a Cloud Library")]
    JoinNotRequired,
    #[error(transparent)]
    Config(#[from] crate::preclude::ConfigError),
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error(transparent)]
    Bootstrap(#[from] CloudLibraryBootstrapError),
    #[error(transparent)]
    Join(#[from] CloudLibraryJoinError),
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
