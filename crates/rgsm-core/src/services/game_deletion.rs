use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::app_dirs::resolve_app_path;
use crate::cloud_sync::CloudSyncSessionConfig;
use crate::cloud_sync::v2::{
    CLOUD_MANIFEST_PATH, CloudManifestRepository, DeletionRegistryRepository,
    DeviceProfileRepository, SharedGameDeletion, SharedGameDeletionOutcome,
    SharedLibraryRepository,
};
use crate::config::{CloudNamespaceGeneration, cloud_bootstrap_inputs, remove_shared_game};

use super::{CloudLibraryServiceError, ServiceContext};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DeletedCloudGameView {
    pub game_id: String,
    pub name: String,
    pub deletion_incomplete: bool,
}

impl ServiceContext {
    pub async fn deleted_cloud_games(
        &self,
    ) -> Result<Vec<DeletedCloudGameView>, CloudLibraryServiceError> {
        converge_local_deleted_games().await?;
        let (_, profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let operator = CloudSyncSessionConfig::from(&local_state.cloud_settings).get_op()?;
        let registry = DeletionRegistryRepository::new(operator.clone(), 3)
            .load()
            .await?;
        let shared = SharedLibraryRepository::new(operator.clone(), 3)
            .load()
            .await?;
        let manifest = CloudManifestRepository::new(operator.clone(), CLOUD_MANIFEST_PATH, 3)
            .load()
            .await?;
        let profiles = DeviceProfileRepository::new(operator.clone(), 3)
            .list()
            .await?;
        let local_root = profile.local_archive_root.as_deref().map(resolve_app_path);
        let mut games = Vec::with_capacity(registry.deleted_games.len());
        for (game_id, deletion) in &registry.deleted_games {
            let name = if deletion.name.is_empty() {
                game_id.clone()
            } else {
                deletion.name.clone()
            };
            let archive_prefix =
                crate::cloud_sync::v2::game_deletion::cloud_game_archive_prefix(game_id)?;
            let cloud_archives_remain = cloud_prefix_has_entries(&operator, &archive_prefix)
                .await
                .map_err(crate::preclude::BackendError::from)?;
            let deletion_incomplete = shared.games.iter().any(|game| game.storage_key == *game_id)
                || manifest.games.contains_key(game_id)
                || profiles.iter().any(|profile| {
                    !registry.deleted_profiles.contains_key(&profile.device.id)
                        && profile.references_game(game_id, &name)
                })
                || cloud_archives_remain
                || local_root
                    .as_ref()
                    .is_some_and(|root| root.join(game_id).exists());
            games.push(DeletedCloudGameView {
                game_id: game_id.clone(),
                name,
                deletion_incomplete,
            });
        }
        Ok(games)
    }

    pub async fn permanently_delete_cloud_game(
        &self,
        game_id: &str,
        confirmed: bool,
    ) -> Result<SharedGameDeletionOutcome, CloudLibraryServiceError> {
        let (library, profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let operator = CloudSyncSessionConfig::from(&local_state.cloud_settings).get_op()?;
        let registry = DeletionRegistryRepository::new(operator.clone(), 3)
            .load()
            .await?;
        let game_name = library
            .games
            .iter()
            .find(|game| game.storage_key == game_id)
            .map(|game| game.name.clone())
            .or_else(|| {
                registry
                    .deleted_games
                    .get(game_id)
                    .map(|deletion| deletion.name.clone())
            })
            .filter(|name| !name.is_empty())
            .ok_or_else(|| CloudLibraryServiceError::GameProfileNotFound(game_id.to_string()))?;
        let local_root = profile
            .local_archive_root
            .as_deref()
            .map(resolve_app_path)
            .ok_or(CloudLibraryServiceError::StorageLocationRequired)?;
        let outcome =
            SharedGameDeletion::new(operator, local_root, local_state.current_device_id, 3)
                .delete(game_id, &game_name, confirmed)
                .await?;
        remove_shared_game(game_id, &game_name)?;
        Ok(outcome)
    }
}

pub(crate) async fn converge_local_deleted_games() -> Result<(), CloudLibraryServiceError> {
    let (_, profile, local_state) = cloud_bootstrap_inputs()?;
    if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
        return Ok(());
    }
    let operator = CloudSyncSessionConfig::from(&local_state.cloud_settings).get_op()?;
    let registry = DeletionRegistryRepository::new(operator, 3).load().await?;
    for (game_id, deletion) in registry.deleted_games {
        if let Some(root) = profile.local_archive_root.as_deref().map(resolve_app_path) {
            crate::cloud_sync::v2::game_deletion::remove_local_game_directory(&root, &game_id)
                .await?;
        }
        let name = if deletion.name.is_empty() {
            game_id.as_str()
        } else {
            deletion.name.as_str()
        };
        remove_shared_game(&game_id, name)?;
    }
    Ok(())
}

async fn cloud_prefix_has_entries(
    operator: &opendal::Operator,
    prefix: &str,
) -> Result<bool, opendal::Error> {
    let mut lister = operator.lister(prefix).await?;
    Ok(lister.try_next().await?.is_some())
}
