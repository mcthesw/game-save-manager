use crate::cloud_sync::CloudSyncSessionConfig;
use crate::cloud_sync::v2::{
    CloudLibraryBootstrap, CloudLibraryJoin, CloudLibraryJoinError, CloudLibraryJoinReview,
    CloudNamespaceClassification, DeletionRegistryError, DeletionRegistryRepository,
    DeviceProfileRepository, GameJoinClassification, JoinGameDecision,
};
use crate::config::{
    CloudNamespaceGeneration, SharedLibrary, cloud_bootstrap_inputs, connect_cloud_library_local,
    connected_cloud_profile, resolve_cloud_definitions_local, resolved_cloud_profile,
};

use super::cloud_library_target::bound_v2_operator;
use super::{
    CloudLibraryJoinOutcome, CloudLibraryServiceError, CloudLibraryStatus, ServiceContext,
};

#[cfg(test)]
#[path = "cloud_connection_tests.rs"]
mod tests;

impl ServiceContext {
    pub(super) fn require_shared_game(
        &self,
        game_id: &str,
    ) -> Result<(), CloudLibraryServiceError> {
        let (library, _, state) = cloud_bootstrap_inputs()?;
        if state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        if state.is_local_game(game_id) {
            return Err(
                crate::cloud_sync::v2::MaterializationError::GameDefinitionNotAccepted(
                    game_id.to_string(),
                )
                .into(),
            );
        }
        if !library.games.iter().any(|game| game.storage_key == game_id) {
            return Err(CloudLibraryServiceError::GameProfileNotFound(
                game_id.to_string(),
            ));
        }
        Ok(())
    }
    /// Connect saved settings to an existing namespace without choosing between
    /// conflicting definitions or transferring any archives/live save files.
    pub async fn connect_cloud_library(
        &self,
    ) -> Result<CloudLibraryStatus, CloudLibraryServiceError> {
        let (library, profile, state) = cloud_bootstrap_inputs()?;
        let operator = CloudSyncSessionConfig::from(&state.cloud_settings).get_op()?;
        let classification = CloudLibraryBootstrap::new(operator.clone(), 3)
            .inspect()
            .await?;
        let CloudNamespaceClassification::SupportedV2 {
            descriptor,
            shared_library,
            ..
        } = classification
        else {
            return self.inspect_cloud_library().await;
        };
        if state.cloud_namespace_generation == CloudNamespaceGeneration::V2 {
            // A different namespace still needs explicit reconnect confirmation.
            bound_v2_operator(&state).await?;
            super::cloud_library_metadata::refresh_shared_library().await?;
        } else {
            let published = connected_cloud_profile(&library, &profile, &state, &shared_library)?;
            // Registration must succeed before local state starts requiring it.
            DeviceProfileRepository::new(operator, 3)
                .publish(&state.current_device_id, &published)
                .await?;
            connect_cloud_library_local(
                &library,
                &profile,
                &state,
                &shared_library,
                &descriptor.library_id,
            )?;
        }
        Ok(CloudLibraryStatus::Active {
            game_count: shared_library.games.len(),
        })
    }

    pub(super) async fn review_pending_definitions(
        &self,
    ) -> Result<CloudLibraryJoinReview, CloudLibraryServiceError> {
        let (library, _, state) = cloud_bootstrap_inputs()?;
        let candidates = SharedLibrary {
            schema_version: library.schema_version,
            games: state
                .local_games
                .iter()
                .filter(|local| {
                    library
                        .games
                        .iter()
                        .any(|remote| remote.storage_key == local.storage_key)
                })
                .cloned()
                .collect(),
        };
        let operator = bound_v2_operator(&state).await?;
        let registry = DeletionRegistryRepository::new(operator.clone(), 3)
            .load()
            .await?;
        let mut review = CloudLibraryJoin::new(operator, 3)
            .review(&candidates)
            .await?;
        // An active library only offers a choice for the same live identity.
        // An absent/deleted cloud definition leaves its local copy independent.
        review.items.retain(|item| {
            !registry.deleted_games.contains_key(&item.local_game_id)
                && matches!(
                    item.classification,
                    GameJoinClassification::Same | GameJoinClassification::GameDefinitionConflict
                )
        });
        Ok(review)
    }

    pub(super) async fn resolve_pending_definitions(
        &self,
        decisions: &[JoinGameDecision],
        confirmed_replacements: bool,
    ) -> Result<CloudLibraryJoinOutcome, CloudLibraryServiceError> {
        let (library, profile, state) = cloud_bootstrap_inputs()?;
        let candidates = SharedLibrary {
            schema_version: library.schema_version,
            games: state
                .local_games
                .iter()
                .filter(|local| {
                    decisions
                        .iter()
                        .any(|decision| decision.local_game_id == local.storage_key)
                })
                .cloned()
                .collect(),
        };
        let operator = bound_v2_operator(&state).await?;
        let registry = DeletionRegistryRepository::new(operator.clone(), 3);
        for decision in decisions {
            match registry
                .ensure_active(&state.current_device_id, &decision.local_game_id)
                .await
            {
                Ok(()) => {}
                Err(DeletionRegistryError::GameDeleted(game_id)) => {
                    let game_name = candidates
                        .games
                        .iter()
                        .find(|game| game.storage_key == game_id)
                        .map(|game| game.name.clone())
                        .unwrap_or(game_id);
                    return Ok(CloudLibraryJoinOutcome::ReviewChanged { game_name });
                }
                Err(error) => return Err(error.into()),
            }
        }
        let joined = match CloudLibraryJoin::new(operator.clone(), 3)
            .update_definitions(&candidates, &profile, decisions, confirmed_replacements)
            .await
        {
            Ok(joined) => joined,
            Err(
                CloudLibraryJoinError::TargetChanged(game_name)
                | CloudLibraryJoinError::DecisionRequired(game_name)
                | CloudLibraryJoinError::LocalGameChanged(game_name),
            ) => return Ok(CloudLibraryJoinOutcome::ReviewChanged { game_name }),
            Err(error) => return Err(error.into()),
        };
        let resolved_ids = decisions
            .iter()
            .map(|decision| decision.local_game_id.clone())
            .collect::<Vec<_>>();
        let published = resolved_cloud_profile(
            &library,
            &profile,
            &state,
            &joined.shared_library,
            &resolved_ids,
        )?;
        // A failed write (including a concurrent remote deletion) must leave the
        // unresolved local version protected and available for another attempt.
        DeviceProfileRepository::new(operator, 3)
            .publish(&state.current_device_id, &published)
            .await?;
        resolve_cloud_definitions_local(
            &library,
            &profile,
            &state,
            &joined.shared_library,
            &resolved_ids,
        )?;
        Ok(CloudLibraryJoinOutcome::Active {
            game_count: joined.shared_library.games.len(),
        })
    }
}
