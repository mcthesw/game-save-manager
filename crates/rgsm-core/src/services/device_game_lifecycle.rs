use serde::{Deserialize, Serialize};
use specta::Type;

use crate::app_dirs::resolve_app_path;
use crate::cloud_sync::CloudSyncSessionConfig;
use crate::cloud_sync::v2::{DeviceProfileRepository, LocalArchiveEviction};
use crate::config::{
    CloudNamespaceGeneration, cloud_bootstrap_inputs, get_config, replace_current_device_profile,
};

use super::{CloudLibraryServiceError, ServiceContext};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DeviceGameStatus {
    pub game_id: String,
    pub managed: bool,
    pub visible: bool,
}

impl ServiceContext {
    pub fn current_device_game_statuses(
        &self,
    ) -> Result<Vec<DeviceGameStatus>, CloudLibraryServiceError> {
        let (library, profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Ok(library
                .games
                .into_iter()
                .map(|game| DeviceGameStatus {
                    game_id: game.storage_key,
                    managed: true,
                    visible: true,
                })
                .collect());
        }
        Ok(library
            .games
            .into_iter()
            .map(|game| {
                let settings = profile.games.get(&game.storage_key);
                DeviceGameStatus {
                    game_id: game.storage_key,
                    managed: settings.is_some(),
                    visible: settings.is_some_and(|settings| settings.visible),
                }
            })
            .collect())
    }

    pub async fn set_device_game_visibility(
        &self,
        game_id: &str,
        visible: bool,
    ) -> Result<DeviceGameStatus, CloudLibraryServiceError> {
        let (library, expected, local_state) = v2_inputs()?;
        if !library.games.iter().any(|game| game.storage_key == game_id) {
            return Err(CloudLibraryServiceError::GameProfileNotFound(
                game_id.to_string(),
            ));
        }
        let mut accepted = expected.clone();
        let settings = accepted
            .games
            .get_mut(game_id)
            .ok_or_else(|| CloudLibraryServiceError::GameNotManaged(game_id.to_string()))?;
        settings.visible = visible;
        publish_profile(&local_state, &expected, &accepted).await?;
        Ok(DeviceGameStatus {
            game_id: game_id.to_string(),
            managed: true,
            visible,
        })
    }

    pub async fn set_device_game_managed(
        &self,
        game_id: &str,
        managed: bool,
        confirmed: bool,
    ) -> Result<DeviceGameStatus, CloudLibraryServiceError> {
        if !managed && !confirmed {
            return Err(CloudLibraryServiceError::ConfirmationRequired);
        }
        let (library, expected, local_state) = v2_inputs()?;
        let shared = library
            .games
            .iter()
            .find(|game| game.storage_key == game_id)
            .ok_or_else(|| CloudLibraryServiceError::GameProfileNotFound(game_id.to_string()))?;
        let mut accepted = expected.clone();
        if managed {
            if !accepted.games.contains_key(game_id) {
                let defaults = expected.for_shared_library(&library);
                let settings = defaults
                    .games
                    .get(game_id)
                    .cloned()
                    .expect("shared Game receives Device defaults");
                accepted.games.insert(game_id.to_string(), settings);
            }
        } else {
            accepted.games.remove(game_id);
            if let Some(game) = get_config()?
                .games
                .into_iter()
                .find(|game| game.storage_key == game_id)
            {
                accepted.quick_action.remove_deleted_game_reference(&game);
            } else if accepted.quick_action.quick_action_game_id.as_deref()
                == Some(shared.storage_key.as_str())
            {
                accepted.quick_action.quick_action_game_id = None;
            }
        }
        publish_profile(&local_state, &expected, &accepted).await?;
        Ok(DeviceGameStatus {
            game_id: game_id.to_string(),
            managed,
            visible: managed,
        })
    }

    pub async fn evict_local_archive(
        &self,
        game_id: &str,
        snapshot_id: &str,
        confirmed: bool,
    ) -> Result<bool, CloudLibraryServiceError> {
        if !confirmed {
            return Err(CloudLibraryServiceError::ConfirmationRequired);
        }
        self.converged_materializer().await?;
        let (_, profile, local_state) = v2_inputs()?;
        let local_archive_root = profile
            .local_archive_root
            .as_deref()
            .map(resolve_app_path)
            .ok_or(CloudLibraryServiceError::StorageLocationRequired)?;
        let session = CloudSyncSessionConfig::from(&local_state.cloud_settings);
        Ok(LocalArchiveEviction::new(
            session.get_op()?,
            local_archive_root,
            local_state.current_device_id,
            3,
        )
        .evict(game_id, snapshot_id)
        .await?)
    }
}

fn v2_inputs() -> Result<
    (
        crate::config::SharedLibrary,
        crate::config::DeviceProfile,
        crate::config::LocalState,
    ),
    CloudLibraryServiceError,
> {
    let inputs = cloud_bootstrap_inputs()?;
    if inputs.2.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
        return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
    }
    Ok(inputs)
}

async fn publish_profile(
    local_state: &crate::config::LocalState,
    expected: &crate::config::DeviceProfile,
    accepted: &crate::config::DeviceProfile,
) -> Result<(), CloudLibraryServiceError> {
    let session = CloudSyncSessionConfig::from(&local_state.cloud_settings);
    DeviceProfileRepository::new(session.get_op()?, 3)
        .publish(&local_state.current_device_id, accepted)
        .await?;
    replace_current_device_profile(expected, accepted)?;
    Ok(())
}
