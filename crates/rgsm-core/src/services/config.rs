use crate::config::{self, Config, get_config};
use crate::hooks::HookSource;
use crate::preclude::ConfigError;

use super::ServiceContext;

impl ServiceContext {
    pub async fn save_config(&self, config: &Config) -> Result<(), ConfigError> {
        config::set_config_keeping_definitions(config)
    }

    pub async fn reset_settings(&self) -> Result<Config, ConfigError> {
        let settings = Config::default().settings;
        let mut config = get_config()?;
        config.settings = settings;
        config::set_config(&config).await?;
        Ok(config)
    }

    pub fn restore_config_backup(&self, index: usize) -> Result<Config, ConfigError> {
        let backup = config::backup::load_config_from_backup(index)?;
        config::replace_config_local(&backup)?;
        get_config()
    }

    pub async fn fire_config_saved(&self, config: Config, source: HookSource) {
        self.pipeline()
            .fire_config_saved(&crate::hooks::ConfigSavedCtx { config, source })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::config::{ConfigTestStateGuard, activate_cloud_namespace_v2, set_config_local};
    use crate::hooks::HookPipeline;

    const LIBRARY_ID: &str = "11111111-1111-4111-8111-111111111111";

    fn connected_config() -> (Config, ConfigTestStateGuard) {
        let config = Config {
            games: serde_json::from_value(serde_json::json!([
                {"name": "First", "storage_key": "first", "save_paths": []},
                {"name": "Second", "storage_key": "second", "save_paths": []}
            ]))
            .unwrap(),
            ..Config::default()
        };
        let guard = ConfigTestStateGuard::replace_with(&config).unwrap();
        set_config_local(&config).unwrap();
        let (library, profile, _) = crate::config::cloud_bootstrap_inputs().unwrap();
        activate_cloud_namespace_v2(&library, &profile, LIBRARY_ID).unwrap();
        (config, guard)
    }

    #[test]
    fn a_stale_settings_save_cannot_redefine_connected_games() {
        let _config_lock = crate::config::lock_config_test_file();
        let (config, _state) = connected_config();
        let service = ServiceContext::new(Arc::new(HookPipeline::new(vec![])));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        let mut stale = config.clone();
        stale.games[0].name = "Stale rename".into();
        stale.games.remove(1);
        stale.settings.locale = "zh_SIMPLIFIED".into();
        runtime.block_on(service.save_config(&stale)).unwrap();

        let saved = get_config().unwrap();
        assert_eq!(saved.settings.locale, "zh_SIMPLIFIED");
        let names = saved
            .games
            .iter()
            .map(|game| game.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["First", "Second"]);
        let (_, _, state) = crate::config::cloud_bootstrap_inputs().unwrap();
        assert!(state.pending_game_metadata.is_empty());
    }

    #[test]
    fn per_device_fields_of_connected_games_still_save() {
        let _config_lock = crate::config::lock_config_test_file();
        let (config, _state) = connected_config();
        let service = ServiceContext::new(Arc::new(HookPipeline::new(vec![])));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        let mut edited = config.clone();
        edited.games[0].game_paths.insert(
            crate::device::get_current_device_id().clone(),
            "D:/Games/first.exe".into(),
        );
        runtime.block_on(service.save_config(&edited)).unwrap();

        let saved = get_config().unwrap();
        assert_eq!(
            saved.games[0]
                .game_paths
                .get(crate::device::get_current_device_id())
                .map(String::as_str),
            Some("D:/Games/first.exe")
        );
    }
}
