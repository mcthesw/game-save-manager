use serde::{Deserialize, Serialize};
use specta::Type;

use crate::app_dirs::resolve_app_path;
use crate::cloud_sync::v2::{CloudArchiveEviction, DeviceProfileRepository, LocalArchiveEviction};
use crate::config::{
    CloudNamespaceGeneration, cloud_bootstrap_inputs, get_config, replace_current_device_profile,
};

use super::{CloudLibraryServiceError, ServiceContext, cloud_library_target::bound_v2_operator};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct DeviceGameStatus {
    pub game_id: String,
    pub shared: bool,
    pub managed: bool,
    pub visible: bool,
}

impl ServiceContext {
    pub fn is_shared_game(&self, identity: &str) -> Result<bool, CloudLibraryServiceError> {
        let config = get_config()?;
        let Some(index) = config.position_game_by_identity(identity) else {
            return Ok(false);
        };
        let game_id = &config.games[index].storage_key;
        let (library, _, local_state) = cloud_bootstrap_inputs()?;
        Ok(
            local_state.cloud_namespace_generation == CloudNamespaceGeneration::V2
                && !local_state.is_local_game(game_id)
                && library
                    .games
                    .iter()
                    .any(|game| &game.storage_key == game_id),
        )
    }

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
                    shared: false,
                    managed: true,
                    visible: true,
                })
                .collect());
        }
        let local_ids = local_state
            .local_games
            .iter()
            .map(|game| game.storage_key.clone())
            .collect::<std::collections::HashSet<_>>();
        Ok(library
            .with_local_games(&local_state.local_games)
            .games
            .into_iter()
            .map(|game| {
                let settings = profile.games.get(&game.storage_key);
                DeviceGameStatus {
                    shared: !local_ids.contains(&game.storage_key),
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
        self.require_shared_game(game_id)?;
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
            shared: true,
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
        self.require_shared_game(game_id)?;
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
            shared: true,
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
        self.require_shared_game(game_id)?;
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
        Ok(LocalArchiveEviction::new(
            bound_v2_operator(&local_state).await?,
            local_archive_root,
            local_state.current_device_id,
            3,
        )
        .evict(game_id, snapshot_id)
        .await?)
    }

    pub async fn evict_cloud_archive(
        &self,
        game_id: &str,
        snapshot_id: &str,
        confirmed: bool,
    ) -> Result<bool, CloudLibraryServiceError> {
        self.require_shared_game(game_id)?;
        if !confirmed {
            return Err(CloudLibraryServiceError::ConfirmationRequired);
        }
        self.converged_materializer().await?;
        let (_, _, local_state) = v2_inputs()?;
        Ok(
            CloudArchiveEviction::new(bound_v2_operator(&local_state).await?, 3)
                .evict(game_id, snapshot_id)
                .await?,
        )
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
    DeviceProfileRepository::new(bound_v2_operator(local_state).await?, 3)
        .publish(
            &local_state.current_device_id,
            &accepted.without_local_games(local_state),
        )
        .await?;
    replace_current_device_profile(expected, accepted)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::config::{
        Config, ConfigTestStateGuard, activate_joined_cloud_library, set_config_local,
    };
    use crate::hooks::HookPipeline;

    #[test]
    fn pending_definition_is_one_local_row_and_cannot_use_cloud_actions() {
        let _config_lock = crate::config::lock_config_test_file();
        let config = Config {
            games: serde_json::from_value(serde_json::json!([
                {"name": "Local version", "storage_key": "conflict", "save_paths": []},
                {"name": "Ready", "storage_key": "ready", "save_paths": []}
            ]))
            .unwrap(),
            ..Config::default()
        };
        let _config_state = ConfigTestStateGuard::replace_with(&config).unwrap();
        set_config_local(&config).unwrap();
        let (before, profile, state) = cloud_bootstrap_inputs().unwrap();
        let mut remote = before.clone();
        remote.games[0].name = "Cloud version".into();
        crate::config::connect_cloud_library_local(
            &before,
            &profile,
            &state,
            &remote,
            "11111111-1111-4111-8111-111111111111",
        )
        .unwrap();
        let service = ServiceContext::new(Arc::new(HookPipeline::new(vec![])));
        let statuses = service.current_device_game_statuses().unwrap();
        assert_eq!(statuses.len(), 2);
        assert_eq!(
            statuses
                .iter()
                .filter(|row| row.game_id == "conflict")
                .count(),
            1
        );
        assert!(!service.is_shared_game("conflict").unwrap());
        assert!(service.is_shared_game("ready").unwrap());
        assert!(service.require_shared_game("conflict").is_err());
        assert!(service.require_shared_game("ready").is_ok());
    }

    #[test]
    fn shared_ownership_resolves_local_ids_before_cloud_display_names() {
        let _config_lock = crate::config::lock_config_test_file();
        let archives = temp_dir::TempDir::new().unwrap();
        let config = Config {
            backup_path: archives.path().to_string_lossy().into_owned(),
            games: serde_json::from_value(serde_json::json!([
                {"name": "Renamed Local", "storage_key": "Old Name", "save_paths": []},
                {"name": "Old Name", "storage_key": "remote-key", "save_paths": []}
            ]))
            .unwrap(),
            ..Config::default()
        };
        let _config_state = ConfigTestStateGuard::replace_with(&config).unwrap();
        set_config_local(&config).unwrap();
        let (before, profile, _) = cloud_bootstrap_inputs().unwrap();
        let mut remote = before.clone();
        remote.games.retain(|game| game.storage_key == "remote-key");
        activate_joined_cloud_library(
            &before,
            &profile,
            &remote,
            &profile.for_shared_library(&remote),
            "11111111-1111-4111-8111-111111111111",
        )
        .unwrap();

        let service = ServiceContext::new(Arc::new(HookPipeline::new(vec![])));
        assert!(!service.is_shared_game("Old Name").unwrap());
        assert!(!service.is_shared_game("Renamed Local").unwrap());
        assert!(service.is_shared_game("remote-key").unwrap());
        assert!(!service.is_shared_game("missing").unwrap());

        for game in &config.games {
            let mut snapshots = crate::backup::GameSnapshots::new(&game.name);
            snapshots.backups.push(
                serde_json::from_value(serde_json::json!({
                    "date": "2026-09-05_12-00-00", "describe": "", "path": "test.zip",
                    "created_by": "Timer"
                }))
                .unwrap(),
            );
            game.set_game_snapshots_info(&snapshots).unwrap();
        }
        // This legacy endpoint accepts display names. Its ownership check must
        // protect the same resolved Game that the metadata operation modifies.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            assert!(
                service
                    .set_snapshot_created_by(
                        "Old Name",
                        "2026-09-05_12-00-00",
                        crate::backup::CreatedBy::Manual,
                        crate::hooks::HookSource::UserManual,
                    )
                    .await
                    .is_err()
            );
            service
                .set_snapshot_created_by(
                    "Renamed Local",
                    "2026-09-05_12-00-00",
                    crate::backup::CreatedBy::Manual,
                    crate::hooks::HookSource::UserManual,
                )
                .await
                .unwrap();
        });
        assert_eq!(
            config.games[0].get_game_snapshots_info().unwrap().backups[0].created_by,
            crate::backup::CreatedBy::Manual
        );
        assert_eq!(
            config.games[1].get_game_snapshots_info().unwrap().backups[0].created_by,
            crate::backup::CreatedBy::Timer
        );
    }
}
