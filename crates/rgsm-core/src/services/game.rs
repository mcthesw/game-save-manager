use anyhow::{Result, anyhow, bail};

use crate::backup::{self, AutoBackupConfig, Game, GameDeviceBinding, GameDraft};
use crate::config::{
    CloudNamespaceGeneration, GameAutomationSettingsDraft, cloud_bootstrap_inputs, get_config,
    set_config,
};
use crate::hooks::{GameAddedCtx, GameDeletedCtx, GameUpdatedCtx, HookSource};

use super::ServiceContext;

impl ServiceContext {
    pub async fn add_game(&self, game: &GameDraft, source: HookSource) -> Result<Game> {
        let config = get_config()?;
        if config
            .games
            .iter()
            .any(|g| g.name.eq_ignore_ascii_case(&game.name))
        {
            bail!("Game '{}' already exists", game.name);
        }

        let saved_game = backup::create_game_backup(game).await?;

        let config = get_config()?;
        let snapshots = saved_game.get_game_snapshots_info()?;
        self.pipeline()
            .fire_game_added(&GameAddedCtx {
                config,
                source,
                game: saved_game.clone(),
                snapshots,
            })
            .await;

        Ok(saved_game)
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
        let index = config
            .games
            .iter()
            .position(|g| g.storage_key == storage_key)
            .ok_or_else(|| anyhow!("Game with storage_key '{}' not found", storage_key))?;

        let previous_game = config.games[index].clone();
        config.bind_legacy_game_references();

        // Check for name collision with a *different* game
        if !previous_game.name.eq_ignore_ascii_case(&draft.name)
            && config
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
        crate::config::FavoriteTreeNode::rename_game_leaves(
            &mut config.favorites,
            &previous_game.storage_key,
            &updated_game.name,
        );
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

    pub async fn delete_game(&self, game: &Game, source: HookSource) -> Result<()> {
        let config = get_config()?;
        let identity = if game.storage_key.is_empty() {
            game.name.as_str()
        } else {
            game.storage_key.as_str()
        };
        let index = config
            .position_game_by_identity(identity)
            .ok_or_else(|| anyhow!("Game '{}' not found", identity))?;
        let current_game = &config.games[index];
        let (library, expected, state) = cloud_bootstrap_inputs()?;
        if state.cloud_namespace_generation == CloudNamespaceGeneration::V2
            && !state.is_local_game(&current_game.storage_key)
            && library
                .games
                .iter()
                .any(|shared| shared.storage_key == current_game.storage_key)
        {
            let mut accepted = expected.clone();
            accepted.remove_game_state(&current_game.storage_key, &current_game.name);
            crate::config::replace_current_device_profile(&expected, &accepted)?;
            // Commit the local opt-out first so a retry cannot restart synchronization.
            // The next cloud refresh publishes this Device's updated profile.
            crate::cloud_sync::v2::game_deletion::remove_local_game_directory(
                &crate::app_dirs::resolve_app_path(&config.backup_path),
                &current_game.storage_key,
            )
            .await?;
        } else {
            current_game.delete_game().await?;
        }
        let config = get_config()?;

        self.pipeline()
            .fire_game_deleted(&GameDeletedCtx {
                config,
                source,
                game_name: current_game.name.clone(),
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
        auto_backup_limit: Option<u32>,
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
        config.games[index].auto_backup = auto_backup.map(|mut timer| {
            timer.max_backup_count = None;
            timer
        });
        config.games[index].auto_backup_limit = auto_backup_limit;
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

#[cfg(test)]
mod delete_tests {
    use std::{fs, sync::Arc};

    use crate::config::{
        Config, ConfigTestStateGuard, activate_cloud_namespace_v2, activate_joined_cloud_library,
        set_config_local,
    };
    use crate::hooks::HookPipeline;

    use super::*;

    fn fixture(root: &std::path::Path) -> Result<(ConfigTestStateGuard, Game)> {
        let game: Game = serde_json::from_value(serde_json::json!({
            "name": "Local game",
            "storage_key": "local-game",
            "save_paths": [],
            "cloud_sync_enabled": false
        }))?;
        let config = Config {
            backup_path: root.join("archives").to_string_lossy().into_owned(),
            games: vec![game.clone()],
            ..Config::default()
        };
        let guard = ConfigTestStateGuard::replace_with(&config)?;
        set_config_local(&config)?;
        fs::create_dir_all(root.join("archives/local-game"))?;
        fs::write(root.join("archives/local-game/backup.zip"), b"local backup")?;
        fs::write(root.join("live.sav"), b"live save")?;
        Ok((guard, game))
    }

    #[test]
    fn deleting_unshared_v2_game_keeps_live_save_and_cloud_library() -> Result<()> {
        let _lock = crate::config::lock_config_test_file();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            let root = temp_dir::TempDir::new()?;
            let (_guard, game) = fixture(root.path())?;
            let (library, profile, _) = cloud_bootstrap_inputs()?;
            let mut remote = library.clone();
            remote.games.clear();
            activate_joined_cloud_library(
                &library,
                &profile,
                &remote,
                &profile.for_shared_library(&remote),
                "test-library",
            )?;
            let service = ServiceContext::new(Arc::new(HookPipeline::new(vec![])));

            service.delete_game(&game, HookSource::UserManual).await?;

            let (shared, _, local) = cloud_bootstrap_inputs()?;
            assert!(shared.games.is_empty());
            assert!(local.local_games.is_empty());
            assert!(get_config()?.games.is_empty());
            assert!(!root.path().join("archives/local-game").exists());
            assert_eq!(fs::read(root.path().join("live.sav"))?, b"live save");
            Ok(())
        })
    }

    #[test]
    fn shared_v2_game_local_delete_preserves_shared_history() -> Result<()> {
        let _lock = crate::config::lock_config_test_file();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            let root = temp_dir::TempDir::new()?;
            let (_guard, game) = fixture(root.path())?;
            let (library, profile, _) = cloud_bootstrap_inputs()?;
            activate_cloud_namespace_v2(&library, &profile, "test-library")?;
            let service = ServiceContext::new(Arc::new(HookPipeline::new(vec![])));

            service.delete_game(&game, HookSource::UserManual).await?;
            let (shared, current, _) = cloud_bootstrap_inputs()?;
            assert_eq!(shared, library);
            assert!(!current.games.contains_key(&game.storage_key));
            assert!(!root.path().join("archives/local-game").exists());
            assert_eq!(fs::read(root.path().join("live.sav"))?, b"live save");
            let mut updated = shared.clone();
            let mut discovered = shared.games[0].clone();
            discovered.storage_key = "newly-shared".into();
            updated.games.push(discovered);
            let refreshed = current.for_updated_library(&shared, &updated);
            assert!(!refreshed.games.contains_key(&game.storage_key));
            assert!(refreshed.games.contains_key("newly-shared"));
            Ok(())
        })
    }
    #[test]
    fn deleting_an_unpublished_offline_game_cancels_its_metadata_publication() -> Result<()> {
        let _lock = crate::config::lock_config_test_file();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                let root = temp_dir::TempDir::new()?;
                let (_guard, _) = fixture(root.path())?;
                let (library, profile, _) = cloud_bootstrap_inputs()?;
                activate_cloud_namespace_v2(&library, &profile, "test-library")?;
                let service = ServiceContext::new(Arc::new(HookPipeline::new(vec![])));
                let draft = serde_json::from_value(
                    serde_json::json!({"name":"Unpublished", "save_paths":[]}),
                )?;
                let game = service.add_game(&draft, HookSource::UserManual).await?;
                assert!(
                    cloud_bootstrap_inputs()?
                        .2
                        .pending_game_metadata
                        .contains_key(&game.storage_key)
                );
                service.delete_game(&game, HookSource::UserManual).await?;
                assert!(
                    !cloud_bootstrap_inputs()?
                        .2
                        .pending_game_metadata
                        .contains_key(&game.storage_key)
                );
                assert!(
                    !get_config()?
                        .games
                        .iter()
                        .any(|item| item.storage_key == game.storage_key)
                );
                assert_eq!(cloud_bootstrap_inputs()?.0, library);
                Ok(())
            })
    }
}
