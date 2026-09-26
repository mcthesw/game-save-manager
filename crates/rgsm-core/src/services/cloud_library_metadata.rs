use crate::cloud_sync::v2::{
    CLOUD_MANIFEST_PATH, CloudManifestRepository, DeletionRegistryRepository,
    DeviceProfileRepository, SharedLibraryRepository,
};
use crate::config::{
    CloudNamespaceGeneration, MetadataDecision, cloud_bootstrap_inputs, reconcile_game_metadata,
};

use super::{CloudLibraryServiceError, cloud_library_target::bound_v2_operator};

/// Publish per-game local edits and accept remote definitions. No local store
/// guard is held across network I/O; the final merge reads current local state.
pub(super) async fn refresh_shared_library() -> Result<(), CloudLibraryServiceError> {
    let (_, _, local_state) = cloud_bootstrap_inputs()?;
    if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
        return Ok(());
    }
    let operator = bound_v2_operator(&local_state).await?;
    let profiles = DeviceProfileRepository::new(operator.clone(), 3)
        .list()
        .await?;
    let Some(published) = profiles
        .iter()
        .find(|profile| profile.device.id == local_state.current_device_id)
    else {
        return Err(CloudLibraryServiceError::DeviceReconnectRequired);
    };
    let repository = SharedLibraryRepository::new(operator.clone(), 3);
    let remote = repository.load().await?;
    let mut accepted = remote.clone();
    let mut completed = Vec::new();
    let mut conflicts = Vec::new();
    let registry = DeletionRegistryRepository::new(operator.clone(), 3)
        .load()
        .await?;
    for (id, edit) in &local_state.pending_game_metadata {
        if registry.deleted_games.contains_key(id) {
            // Existing deletion convergence owns local cleanup; never republish.
            continue;
        }
        let existing = remote.games.iter().find(|game| game.storage_key == *id);
        match edit.decision(existing) {
            MetadataDecision::Conflict => conflicts.push(id.clone()),
            MetadataDecision::Accept => completed.push(id.clone()),
            MetadataDecision::Publish => {
                let mut desired = edit.desired.clone();
                desired.snapshot_retention = existing.and_then(|game| game.snapshot_retention);
                if let Some(game) = accepted
                    .games
                    .iter_mut()
                    .find(|game| game.storage_key == *id)
                {
                    *game = desired;
                } else {
                    accepted.games.push(desired);
                }
                completed.push(id.clone());
            }
        }
    }
    if accepted != remote {
        accepted = repository.compare_replace(&remote, &accepted).await?;
    }
    // Idempotent after an interrupted publication, including an empty new Game.
    if !completed.is_empty() {
        let ids = completed.clone();
        CloudManifestRepository::new(operator.clone(), CLOUD_MANIFEST_PATH, 3)
            .mutate(move |manifest| {
                for id in &ids {
                    manifest.game_mut(id);
                }
                Ok(())
            })
            .await?;
    }
    reconcile_game_metadata(&local_state, &accepted, &completed, &conflicts)?;
    let (_, current, state) = cloud_bootstrap_inputs()?;
    let desired = current.without_local_games(&state);
    if *published != desired {
        DeviceProfileRepository::new(operator, 3)
            .publish(&state.current_device_id, &desired)
            .await?;
    }
    Ok(())
}
