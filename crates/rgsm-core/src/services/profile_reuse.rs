use super::{
    ServiceContext, cloud_error_message::logged_cloud_error_message,
    cloud_library_target::bound_v2_operator,
};
use crate::cloud_sync::v2::{DeletionRegistryRepository, DeviceProfileRepository};
use crate::config::{
    cloud_bootstrap_inputs, replace_current_device_profile, reuse_profile_locations,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProfileReuseError {
    #[error("{}", rust_i18n::t!("cloud_join.reuse.confirm_required"))]
    ConfirmationRequired,
    #[error("{}", rust_i18n::t!("cloud_join.reuse.own_device"))]
    OwnDevice,
    #[error("{}", rust_i18n::t!("cloud_join.reuse.unavailable"))]
    ProfileUnavailable,
    #[error("{}", rust_i18n::t!("cloud_join.reuse.local_settings", reason = reason))]
    LocalSettings { reason: String },
    #[error("{}", rust_i18n::t!("cloud_join.reuse.connection", reason = reason))]
    Connection { reason: String },
    #[error("{}", rust_i18n::t!("cloud_join.reuse.publish_failed", reason = reason))]
    PublishFailed { reason: String },
    #[error("{}", rust_i18n::t!("cloud_join.reuse.local_save_failed", reason = reason))]
    LocalSaveFailed { reason: String },
}

impl ProfileReuseError {
    fn local_settings(error: impl std::error::Error + 'static) -> Self {
        Self::LocalSettings {
            reason: logged_cloud_error_message("Reading local settings to reuse a profile", &error),
        }
    }

    fn connection(error: impl std::error::Error + 'static) -> Self {
        Self::Connection {
            reason: logged_cloud_error_message(
                "Reading the Cloud Library to reuse a profile",
                &error,
            ),
        }
    }

    fn publish_failed(error: impl std::error::Error + 'static) -> Self {
        Self::PublishFailed {
            reason: logged_cloud_error_message("Publishing a reused profile", &error),
        }
    }

    fn local_save_failed(error: impl std::error::Error + 'static) -> Self {
        Self::LocalSaveFailed {
            reason: logged_cloud_error_message("Saving a reused profile", &error),
        }
    }
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
            cloud_bootstrap_inputs().map_err(ProfileReuseError::local_settings)?;
        if device_id == state.current_device_id {
            return Err(ProfileReuseError::OwnDevice);
        }
        let operator = bound_v2_operator(&state)
            .await
            .map_err(ProfileReuseError::connection)?;
        let registry = DeletionRegistryRepository::new(operator.clone(), 3)
            .load()
            .await
            .map_err(ProfileReuseError::connection)?;
        if registry.deleted_profiles.contains_key(device_id) {
            return Err(ProfileReuseError::ProfileUnavailable);
        }
        let repository = DeviceProfileRepository::new(operator, 3);
        let source = repository
            .list()
            .await
            .map_err(ProfileReuseError::connection)?
            .into_iter()
            .find(|profile| profile.device.id == device_id)
            .ok_or(ProfileReuseError::ProfileUnavailable)?;
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
            .map_err(ProfileReuseError::publish_failed)?;
        replace_current_device_profile(&current, &accepted)
            .map_err(ProfileReuseError::local_save_failed)?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preclude::ConfigError;

    #[test]
    fn each_failure_keeps_its_own_cause_in_the_message() {
        let denied = opendal::Error::new(opendal::ErrorKind::PermissionDenied, "401");
        let connection = ProfileReuseError::connection(denied).to_string();
        assert!(connection.contains(&*rust_i18n::t!("error.cloud_access_denied")));

        let local = ProfileReuseError::local_settings(ConfigError::StoreLockPoisoned);
        assert!(
            local
                .to_string()
                .contains(&ConfigError::StoreLockPoisoned.to_string())
        );

        let messages = [
            connection,
            local.to_string(),
            ProfileReuseError::OwnDevice.to_string(),
            ProfileReuseError::ProfileUnavailable.to_string(),
        ];
        for (index, message) in messages.iter().enumerate() {
            assert!(!messages[..index].contains(message), "{message}");
        }
    }
}
