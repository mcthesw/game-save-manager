use crate::config::{self, Config, get_config};
use crate::hooks::HookSource;
use crate::preclude::ConfigError;

use super::ServiceContext;

impl ServiceContext {
    pub async fn save_config(&self, config: &Config) -> Result<(), ConfigError> {
        config::set_config(config).await
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
