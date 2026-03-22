use anyhow::{Result, anyhow, bail};

use crate::backup::{self, AutoBackupConfig, Game};
use crate::config::{get_config, set_config};
use crate::hooks::{GameAddedCtx, GameDeletedCtx, GameUpdatedCtx, HookSource};

use super::ServiceContext;

impl ServiceContext {
    pub async fn add_game(
        &self,
        game: &crate::backup::GameDraft,
        source: HookSource,
    ) -> Result<()> {
        let previous_game = get_config()?
            .games
            .iter()
            .find(|existing| existing.name == game.name)
            .cloned();

        backup::create_game_backup(game).await?;

        let config = get_config()?;
        let saved_game = config
            .games
            .iter()
            .find(|existing| existing.name == game.name)
            .cloned()
            .ok_or_else(|| anyhow!("Game '{}' was not found after save", game.name))?;

        if let Some(previous_game) = previous_game {
            self.pipeline()
                .fire_game_updated(&GameUpdatedCtx {
                    config,
                    source,
                    previous_game,
                    game: saved_game,
                })
                .await;
        } else {
            let snapshots = saved_game.get_game_snapshots_info()?;
            self.pipeline()
                .fire_game_added(&GameAddedCtx {
                    config,
                    source,
                    game: saved_game,
                    snapshots,
                })
                .await;
        }

        Ok(())
    }

    pub async fn delete_game(&self, game: &Game, source: HookSource) -> Result<()> {
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
        game_name: &str,
        auto_backup: Option<AutoBackupConfig>,
        source: HookSource,
    ) -> Result<()> {
        if let Some(cfg) = &auto_backup
            && cfg.interval_secs == 0
        {
            bail!("Auto-backup interval_secs must be greater than 0");
        }

        let mut config = get_config()?;
        let game = config
            .games
            .iter_mut()
            .find(|game| game.name == game_name)
            .ok_or_else(|| anyhow!("Game '{}' not found", game_name))?;

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
}
