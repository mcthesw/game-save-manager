use anyhow::{Context, Result, anyhow, bail};

use crate::backup::{self, AutoBackupConfig, Game, GameDeviceBinding, GameDraft};
use crate::cloud_sync::CloudSyncSessionConfig;
use crate::cloud_sync::v2::{
    CLOUD_MANIFEST_PATH, CloudManifestRepository, DeviceProfileRepository, SharedLibraryRepository,
};
use crate::config::{
    CloudNamespaceGeneration, Config, DeviceProfile, GameAutomationSettingsDraft, LocalState,
    SharedLibrary, cloud_bootstrap_inputs, cloud_namespace_generation, get_config, set_config,
    set_config_local,
};
use crate::hooks::{GameAddedCtx, GameDeletedCtx, GameUpdatedCtx, HookSource};

use super::ServiceContext;

impl ServiceContext {
    pub async fn add_game(&self, game: &GameDraft, source: HookSource) -> Result<()> {
        let config = get_config()?;
        if config
            .games
            .iter()
            .any(|g| g.name.eq_ignore_ascii_case(&game.name))
        {
            bail!("Game '{}' already exists", game.name);
        }
        let previous_config = config;
        let v2_change = capture_v2_game_change()?;

        backup::create_game_backup(game).await?;
        if let Some(expected) = v2_change
            && let Err(error) = publish_v2_game_change(expected).await
        {
            rollback_local_game_change(&previous_config, &error)?;
            return Err(error);
        }

        let config = get_config()?;
        let saved_game = config
            .games
            .iter()
            .find(|existing| existing.name.eq_ignore_ascii_case(&game.name))
            .cloned()
            .ok_or_else(|| anyhow!("Game '{}' was not found after save", game.name))?;

        let snapshots = saved_game.get_game_snapshots_info()?;
        self.pipeline()
            .fire_game_added(&GameAddedCtx {
                config,
                source,
                game: saved_game,
                snapshots,
            })
            .await;

        Ok(())
    }

    /// Update an existing game identified by `storage_key`.
    ///
    /// The `storage_key` is used to locate the game in the config. The `draft`
    /// carries the (possibly renamed) display name, save paths, and game paths.
    /// The game's storage identity (backup dirs, cloud paths) is preserved.
    pub async fn update_game(
        &self,
        storage_key: &str,
        draft: &GameDraft,
        source: HookSource,
    ) -> Result<()> {
        let mut config = get_config()?;
        let previous_config = config.clone();
        let v2_change = capture_v2_game_change()?;
        let index = config
            .games
            .iter()
            .position(|g| g.storage_key == storage_key)
            .ok_or_else(|| anyhow!("Game with storage_key '{}' not found", storage_key))?;

        let previous_game = config.games[index].clone();

        // Check for name collision with a *different* game
        if config
            .games
            .iter()
            .any(|g| g.storage_key != storage_key && g.name.eq_ignore_ascii_case(&draft.name))
        {
            bail!("Another game with name '{}' already exists", draft.name);
        }

        config.games[index] = draft.clone().into_game(Some(&previous_game));

        let updated_game = config.games[index].clone();
        config
            .quick_action
            .sync_updated_game_reference(&previous_game, &updated_game);
        set_config(&config).await?;
        if let Some(expected) = v2_change
            && let Err(error) = publish_v2_game_change(expected).await
        {
            rollback_local_game_change(&previous_config, &error)?;
            return Err(error);
        }

        self.pipeline()
            .fire_game_updated(&GameUpdatedCtx {
                config,
                source,
                previous_game,
                game: updated_game,
            })
            .await;

        Ok(())
    }

    pub async fn delete_game(&self, game: &Game, source: HookSource) -> Result<()> {
        if cloud_namespace_generation()? == CloudNamespaceGeneration::V2 {
            bail!(
                "V2 requires the distinct Stop Managing or Permanent Shared Game Deletion action"
            );
        }
        let deleted = game.delete_game().await?;
        let config = get_config()?;

        self.pipeline()
            .fire_game_deleted(&GameDeletedCtx {
                config,
                source,
                game_name: game.name.clone(),
                remote_game_dir_path: deleted.remote_game_dir_path,
            })
            .await;

        Ok(())
    }

    pub async fn set_game_auto_backup(
        &self,
        identity: &str,
        auto_backup: Option<AutoBackupConfig>,
        source: HookSource,
    ) -> Result<()> {
        validate_auto_backup_config(auto_backup.as_ref())?;

        let mut config = get_config()?;
        let index = config
            .position_game_by_identity(identity)
            .ok_or_else(|| anyhow!("Game '{}' not found", identity))?;

        let game = &mut config.games[index];
        let previous_game = game.clone();
        game.auto_backup = auto_backup;
        let updated_game = game.clone();

        set_config(&config).await?;

        self.pipeline()
            .fire_game_updated(&GameUpdatedCtx {
                config,
                source,
                previous_game,
                game: updated_game,
            })
            .await;

        Ok(())
    }

    pub async fn set_game_automation(
        &self,
        storage_key: &str,
        automation: Option<GameAutomationSettingsDraft>,
        source: HookSource,
    ) -> Result<()> {
        validate_game_automation_config(automation.as_ref())?;

        let mut config = get_config()?;
        let index = config
            .position_game_by_identity(storage_key)
            .ok_or_else(|| anyhow!("Game with storage_key '{}' not found", storage_key))?;
        let game = config.games[index].clone();
        let previous_game = game.clone();

        match automation {
            Some(automation) => config
                .quick_action
                .upsert_game_automation(&game, automation),
            None => {
                config.quick_action.remove_game_automation(&game);
            }
        }

        set_config(&config).await?;

        self.pipeline()
            .fire_game_updated(&GameUpdatedCtx {
                config,
                source,
                previous_game,
                game,
            })
            .await;

        Ok(())
    }

    pub async fn set_game_auto_save_settings(
        &self,
        identity: &str,
        auto_backup: Option<AutoBackupConfig>,
        automation: Option<GameAutomationSettingsDraft>,
        source: HookSource,
    ) -> Result<()> {
        validate_auto_backup_config(auto_backup.as_ref())?;
        validate_game_automation_config(automation.as_ref())?;

        let mut config = get_config()?;
        let index = config
            .position_game_by_identity(identity)
            .ok_or_else(|| anyhow!("Game '{}' not found", identity))?;

        let previous_game = config.games[index].clone();
        config.games[index].auto_backup = auto_backup;
        let updated_game = config.games[index].clone();

        match automation {
            Some(automation) => config
                .quick_action
                .upsert_game_automation(&updated_game, automation),
            None => {
                config.quick_action.remove_game_automation(&updated_game);
            }
        }

        set_config(&config).await?;

        self.pipeline()
            .fire_game_updated(&GameUpdatedCtx {
                config,
                source,
                previous_game,
                game: updated_game,
            })
            .await;

        Ok(())
    }

    pub async fn set_game_device_binding(
        &self,
        identity: &str,
        binding: GameDeviceBinding,
        source: HookSource,
    ) -> Result<()> {
        let mut config = get_config()?;
        let index = config
            .position_game_by_identity(identity)
            .ok_or_else(|| anyhow!("Game '{}' not found", identity))?;
        let previous_game = config.games[index].clone();
        config.games[index]
            .device_bindings
            .insert(crate::device::get_current_device_id().clone(), binding);
        let game = config.games[index].clone();
        set_config(&config).await?;
        self.pipeline()
            .fire_game_updated(&GameUpdatedCtx {
                config,
                source,
                previous_game,
                game,
            })
            .await;
        Ok(())
    }

    pub async fn save_restore_mapping(
        &self,
        identity: &str,
        save_unit_id: u32,
        source_dimensions: crate::path_resolution::CandidateDimensions,
        target_candidate_ids: Vec<String>,
        source: HookSource,
    ) -> Result<()> {
        let mut config = get_config()?;
        let index = config
            .position_game_by_identity(identity)
            .ok_or_else(|| anyhow!("Game '{}' not found", identity))?;
        let previous_game = config.games[index].clone();
        let binding = config.games[index]
            .device_bindings
            .entry(crate::device::get_current_device_id().clone())
            .or_default();
        binding.restore_mappings.retain(|rule| {
            rule.save_unit_id != save_unit_id || rule.source_dimensions != source_dimensions
        });
        binding
            .restore_mappings
            .push(crate::backup::RestoreMappingRule {
                save_unit_id,
                source_dimensions,
                target_candidate_ids,
            });
        let game = config.games[index].clone();
        set_config(&config).await?;
        self.pipeline()
            .fire_game_updated(&GameUpdatedCtx {
                config,
                source,
                previous_game,
                game,
            })
            .await;
        Ok(())
    }
}

struct ExpectedV2GameChange {
    library: SharedLibrary,
    profile: DeviceProfile,
    local_state: LocalState,
}

fn capture_v2_game_change() -> Result<Option<ExpectedV2GameChange>> {
    let (library, profile, local_state) = cloud_bootstrap_inputs()?;
    Ok(
        (local_state.cloud_namespace_generation == CloudNamespaceGeneration::V2).then_some(
            ExpectedV2GameChange {
                library,
                profile,
                local_state,
            },
        ),
    )
}

async fn publish_v2_game_change(expected: ExpectedV2GameChange) -> Result<()> {
    let (accepted_library, accepted_profile, accepted_state) = cloud_bootstrap_inputs()?;
    if accepted_state.cloud_namespace_generation != CloudNamespaceGeneration::V2
        || accepted_state.current_device_id != expected.local_state.current_device_id
    {
        bail!("Cloud Library ownership changed while the Game was being saved");
    }

    let session = CloudSyncSessionConfig::from(&expected.local_state.cloud_settings);
    let operator = session.get_op()?;
    let shared = SharedLibraryRepository::new(operator.clone(), 3);
    let committed_library = shared
        .compare_replace(&expected.library, &accepted_library)
        .await
        .context("failed to publish the updated V2 Shared Library")?;
    let profiles = DeviceProfileRepository::new(operator, 3);
    if let Err(error) = profiles
        .publish(&accepted_state.current_device_id, &accepted_profile)
        .await
    {
        let profile_rollback = profiles
            .publish(&accepted_state.current_device_id, &expected.profile)
            .await;
        let library_rollback = shared
            .compare_replace(&committed_library, &expected.library)
            .await;
        bail!(
            "failed to publish the updated V2 Device Profile: {error}; Device Profile rollback: {}; Shared Library rollback: {}",
            profile_rollback
                .err()
                .map_or_else(|| "completed".to_string(), |reason| reason.to_string()),
            library_rollback
                .err()
                .map_or_else(|| "completed".to_string(), |reason| reason.to_string())
        );
    }
    let added_game_ids = accepted_library
        .games
        .iter()
        .filter(|game| {
            !expected
                .library
                .games
                .iter()
                .any(|previous| previous.storage_key == game.storage_key)
        })
        .map(|game| game.storage_key.clone())
        .collect::<Vec<_>>();
    if !added_game_ids.is_empty() {
        let session = CloudSyncSessionConfig::from(&expected.local_state.cloud_settings);
        let manifest = CloudManifestRepository::new(session.get_op()?, CLOUD_MANIFEST_PATH, 3);
        if let Err(error) = manifest
            .mutate(move |manifest| {
                for game_id in &added_game_ids {
                    manifest.game_mut(game_id);
                }
                Ok(())
            })
            .await
        {
            let profile_rollback = profiles
                .publish(&accepted_state.current_device_id, &expected.profile)
                .await;
            let library_rollback = shared
                .compare_replace(&committed_library, &expected.library)
                .await;
            bail!(
                "failed to initialize the updated V2 Cloud Manifest: {error}; Device Profile rollback: {}; Shared Library rollback: {}",
                profile_rollback
                    .err()
                    .map_or_else(|| "completed".to_string(), |reason| reason.to_string()),
                library_rollback
                    .err()
                    .map_or_else(|| "completed".to_string(), |reason| reason.to_string())
            );
        }
    }
    Ok(())
}

fn rollback_local_game_change(previous: &Config, operation: &anyhow::Error) -> Result<()> {
    set_config_local(previous).with_context(|| {
        format!("{operation}; additionally failed to roll back the local Game definition")
    })
}

fn validate_auto_backup_config(auto_backup: Option<&AutoBackupConfig>) -> Result<()> {
    if let Some(cfg) = auto_backup
        && cfg.interval_secs == 0
    {
        bail!("Auto-backup interval_secs must be greater than 0");
    }

    Ok(())
}

fn validate_game_automation_config(automation: Option<&GameAutomationSettingsDraft>) -> Result<()> {
    if let Some(automation) = automation
        && let Some(interval_secs) = automation.in_process_interval_secs
        && interval_secs == 0
    {
        bail!("Process monitor interval_secs must be greater than 0");
    }

    Ok(())
}
