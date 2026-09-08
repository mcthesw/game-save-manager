use super::{ServiceContext, cloud_library_target::bound_v2_operator};
use crate::cloud_sync::v2::{DeletionRegistryRepository, DeviceProfileRepository};
use crate::config::{
    cloud_bootstrap_inputs, replace_current_device_profile, reuse_profile_locations,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProfileReuseError {
    #[error("{}", rust_i18n::t!("cloud_join.reuse.confirm_required"))]
    ConfirmationRequired,
    #[error("{}", rust_i18n::t!("cloud_join.reuse.unavailable"))]
    Unavailable,
    #[error("{}", rust_i18n::t!("cloud_join.reuse.save_failed"))]
    SaveFailed,
}

impl ServiceContext {
    pub async fn reuse_cloud_device_locations(
        &self,
        device_id: &str,
        confirmed: bool,
    ) -> Result<usize, ProfileReuseError> {
        if !confirmed {
            return Err(ProfileReuseError::ConfirmationRequired);
        }
        let (library, current, state) =
            cloud_bootstrap_inputs().map_err(|_| ProfileReuseError::Unavailable)?;
        if device_id == state.current_device_id {
            return Err(ProfileReuseError::Unavailable);
        }
        let operator = bound_v2_operator(&state)
            .await
            .map_err(|_| ProfileReuseError::Unavailable)?;
        let registry = DeletionRegistryRepository::new(operator.clone(), 3)
            .load()
            .await
            .map_err(|_| ProfileReuseError::Unavailable)?;
        if registry.deleted_profiles.contains_key(device_id) {
            return Err(ProfileReuseError::Unavailable);
        }
        let repository = DeviceProfileRepository::new(operator, 3);
        let source = repository
            .list()
            .await
            .map_err(|_| ProfileReuseError::Unavailable)?
            .into_iter()
            .find(|profile| profile.device.id == device_id)
            .ok_or(ProfileReuseError::Unavailable)?;
        let games = library
            .games
            .into_iter()
            .filter(|game| {
                !state.is_local_game(&game.storage_key)
                    && !registry.deleted_games.contains_key(&game.storage_key)
            })
            .collect::<Vec<_>>();
        let (accepted, count) = reuse_profile_locations(&current, &source, &games);
        repository
            .publish(
                &state.current_device_id,
                &accepted.without_local_games(&state),
            )
            .await
            .map_err(|_| ProfileReuseError::SaveFailed)?;
        replace_current_device_profile(&current, &accepted)
            .map_err(|_| ProfileReuseError::SaveFailed)?;
        Ok(count)
    }
}
