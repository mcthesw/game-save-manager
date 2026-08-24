use serde::{Deserialize, Serialize};
use specta::Type;

use crate::app_dirs::resolve_app_path;
use crate::cloud_sync::v2::{
    CLOUD_MANIFEST_PATH, CloudManifestRepository, DeviceProfileRepository, ManifestError,
    SharedLibraryRepository, SnapshotState, SnapshotSyncCoordinator,
};
use crate::config::{
    CloudNamespaceGeneration, SharedSnapshotRetentionPolicy, accept_remote_shared_library,
    cloud_bootstrap_inputs, get_config, replace_shared_library,
};

use super::sync::CloudLibraryServiceError;
use super::{ServiceContext, cloud_library_target::bound_v2_operator};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct SnapshotRetentionOutcome {
    pub limit: Option<u32>,
    pub deleted: usize,
}

/// Refresh the authoritative Shared Library for a Device that is still a
/// registered member of the remote library. Device-local settings are rebased
/// onto the accepted portable definitions.
pub(crate) async fn refresh_v2_snapshot_retention() -> Result<(), CloudLibraryServiceError> {
    let (expected_library, expected_profile, local_state) = cloud_bootstrap_inputs()?;
    if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
        return Ok(());
    }
    let operator = bound_v2_operator(&local_state).await?;
    if !DeviceProfileRepository::new(operator.clone(), 3)
        .list()
        .await?
        .iter()
        .any(|profile| profile.device.id == local_state.current_device_id)
    {
        return Err(CloudLibraryServiceError::DeviceReconnectRequired);
    }
    let remote = SharedLibraryRepository::new(operator.clone(), 3)
        .load()
        .await?;
    if remote != expected_library {
        let accepted_profile = expected_profile.for_shared_library(&remote);
        DeviceProfileRepository::new(operator, 3)
            .publish(&local_state.current_device_id, &accepted_profile)
            .await?;
        accept_remote_shared_library(
            &expected_library,
            &expected_profile,
            &remote,
            &accepted_profile,
            local_state
                .cloud_library_id
                .as_deref()
                .ok_or(CloudLibraryServiceError::ActiveLibraryUnavailable)?,
        )?;
    }
    Ok(())
}

impl ServiceContext {
    pub async fn set_shared_snapshot_retention(
        &self,
        game_id: &str,
        limit: Option<u32>,
        confirmed: bool,
    ) -> Result<SnapshotRetentionOutcome, CloudLibraryServiceError> {
        if limit == Some(0) {
            return Err(CloudLibraryServiceError::InvalidRetentionLimit);
        }
        let (expected_library, _, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let previous = expected_library
            .games
            .iter()
            .find(|game| game.storage_key == game_id)
            .ok_or_else(|| CloudLibraryServiceError::GameProfileNotFound(game_id.to_string()))?
            .snapshot_retention
            .map(|policy| policy.automatic_snapshots_per_branch);
        let risky = limit.is_some()
            && (previous.is_none() || limit.zip(previous).is_some_and(|(new, old)| new < old));
        if risky && !confirmed {
            return Err(CloudLibraryServiceError::RetentionConfirmationRequired);
        }

        if previous != limit {
            let mut accepted_library = expected_library.clone();
            let game = accepted_library
                .games
                .iter_mut()
                .find(|game| game.storage_key == game_id)
                .expect("game was found in the expected Shared Library");
            game.snapshot_retention =
                limit.map(
                    |automatic_snapshots_per_branch| SharedSnapshotRetentionPolicy {
                        automatic_snapshots_per_branch,
                    },
                );
            let accepted_library =
                SharedLibraryRepository::new(bound_v2_operator(&local_state).await?, 3)
                    .compare_replace(&expected_library, &accepted_library)
                    .await?;
            replace_shared_library(&expected_library, &accepted_library)?;
        }

        let deleted = match limit {
            Some(limit) => self.enforce_retention_now(game_id, limit).await?,
            None => 0,
        };
        Ok(SnapshotRetentionOutcome { limit, deleted })
    }

    pub async fn set_snapshot_retention_protected(
        &self,
        game_id: &str,
        snapshot_id: &str,
        protected: bool,
        confirmed: bool,
    ) -> Result<SnapshotRetentionOutcome, CloudLibraryServiceError> {
        if !protected && !confirmed {
            return Err(CloudLibraryServiceError::ProtectionRemovalConfirmationRequired);
        }
        let (library, _, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let limit = library
            .games
            .iter()
            .find(|game| game.storage_key == game_id)
            .ok_or_else(|| CloudLibraryServiceError::GameProfileNotFound(game_id.to_string()))?
            .snapshot_retention
            .map(|policy| policy.automatic_snapshots_per_branch);
        let game_id_owned = game_id.to_string();
        let snapshot_id_owned = snapshot_id.to_string();
        CloudManifestRepository::new(
            bound_v2_operator(&local_state).await?,
            CLOUD_MANIFEST_PATH,
            3,
        )
        .mutate(move |manifest| {
            let game = manifest
                .games
                .get_mut(&game_id_owned)
                .ok_or_else(|| ManifestError::MissingGame(game_id_owned.clone()))?;
            let node = game
                .snapshots
                .get_mut(&snapshot_id_owned)
                .ok_or_else(|| ManifestError::MissingSnapshot(snapshot_id_owned.clone()))?;
            let SnapshotState::Live(live) = &mut node.state else {
                return Err(ManifestError::ExpectedLive(snapshot_id_owned.clone()));
            };
            live.retention_protected = protected;
            Ok(())
        })
        .await?;
        let deleted = match (protected, limit) {
            (false, Some(limit)) => self.enforce_retention_now(game_id, limit).await?,
            _ => 0,
        };
        Ok(SnapshotRetentionOutcome { limit, deleted })
    }

    async fn retention_coordinator(
        &self,
    ) -> Result<SnapshotSyncCoordinator, CloudLibraryServiceError> {
        let (_, profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let local_archive_root = profile
            .local_archive_root
            .as_deref()
            .map(resolve_app_path)
            .ok_or(CloudLibraryServiceError::StorageLocationRequired)?;
        Ok(SnapshotSyncCoordinator::new(
            bound_v2_operator(&local_state).await?,
            local_archive_root,
            local_state.current_device_id,
            resolve_app_path("GameSaveManager.cloud-v2-materialization.json"),
            3,
        ))
    }

    async fn enforce_retention_now(
        &self,
        game_id: &str,
        limit: u32,
    ) -> Result<usize, CloudLibraryServiceError> {
        let outcome = self
            .retention_coordinator()
            .await?
            .enforce_retention(game_id, limit)
            .await?;
        if !outcome.tombstones.is_empty()
            && let Some(game) = get_config()?
                .games
                .into_iter()
                .find(|game| game.storage_key == game_id)
        {
            game.forget_v2_tombstones(&outcome.tombstones)?;
        }
        Ok(outcome.deleted)
    }
}
