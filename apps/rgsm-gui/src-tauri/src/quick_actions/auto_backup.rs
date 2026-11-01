use log::{info, warn};
use tauri::{AppHandle, Manager};

use rgsm_core::backup::{AutoBackupConfig, Game, TimerSnapshotDecision};
use rgsm_core::config::get_config;
use rgsm_core::services::ServiceContext;

use super::{
    QuickActionOperation, QuickActionType, notify_backup_skipped_unchanged,
    notify_quick_action_failed,
};

/// Timers and the process monitor keep their own copy of a game. The stored
/// definition wins at trigger time, and a game that no longer exists (for
/// example removed by another device) must not be backed up again.
fn current_game<'a>(games: &'a [Game], scheduled: &Game) -> Option<&'a Game> {
    games
        .iter()
        .find(|game| game.storage_key == scheduled.storage_key)
}

pub async fn perform_changed_auto_backup(
    app: &AppHandle,
    scheduled_game: &Game,
    retention: Option<&AutoBackupConfig>,
    trigger: QuickActionType,
) {
    let Some(_operation) = app.state::<crate::app_operations::AppOperations>().begin() else {
        warn!(
            target: "rgsm::quick_action::auto_backup",
            "Skipped automatic backup for '{}': an application update is being installed",
            scheduled_game.name
        );
        return;
    };
    let game = match get_config() {
        Ok(config) => match current_game(&config.games, scheduled_game) {
            Some(game) => game.clone(),
            None => {
                info!(
                    target: "rgsm::quick_action::auto_backup",
                    "Skipped automatic backup for '{}': the game no longer exists",
                    scheduled_game.name
                );
                return;
            }
        },
        Err(err) => {
            warn!(
                target: "rgsm::quick_action::auto_backup",
                "Failed to load config before automatic backup: {err:?}"
            );
            return;
        }
    };
    let game = &game;
    let describe = trigger.generate_describe();
    let created_by = trigger.to_created_by();

    let service = ServiceContext::new(app.state::<crate::hooks::HookPipelineState>().snapshot());
    match service
        .create_snapshot_if_changed(game, &describe, created_by, trigger.to_hook_source(), None)
        .await
    {
        Ok(TimerSnapshotDecision::SkippedUnchanged) => {
            info!(
                target: "rgsm::quick_action::auto_backup",
                "Skipped automatic backup for '{}': state unchanged",
                game.name
            );
            if let Ok(config) = get_config() {
                notify_backup_skipped_unchanged(app, &config, trigger, game);
            }
            return;
        }
        Ok(TimerSnapshotDecision::Created) => {
            info!(
                target: "rgsm::quick_action::auto_backup",
                "Automatic backup created for '{}'",
                game.name
            );
        }
        Err(err) => {
            warn!(
                target: "rgsm::quick_action::auto_backup",
                "Automatic backup failed for '{}': {err:?}",
                game.name
            );
            if let Ok(config) = get_config() {
                notify_quick_action_failed(
                    app,
                    &config,
                    trigger,
                    QuickActionOperation::Backup,
                    game,
                    &err.to_string(),
                );
            }
            return;
        }
    }

    let config = match get_config() {
        Ok(config) => config,
        Err(err) => {
            warn!(
                target: "rgsm::quick_action::auto_backup",
                "Failed to load config after automatic backup: {err:?}"
            );
            return;
        }
    };

    let effective_max = game
        .auto_backup_limit
        .or_else(|| retention.and_then(|retention| retention.max_backup_count))
        .unwrap_or(config.settings.max_auto_backup_count);
    if effective_max == 0 {
        return;
    }

    if let Err(err) = service
        .cleanup_old_auto_backups(game, effective_max, trigger.to_hook_source())
        .await
    {
        warn!(
            target: "rgsm::quick_action::auto_backup",
            "Automatic backup cleanup failed for '{}': {err:?}",
            game.name
        );
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn game(name: &str, storage_key: &str) -> Game {
        Game {
            name: name.to_string(),
            storage_key: storage_key.to_string(),
            save_paths: vec![],
            game_paths: HashMap::new(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            auto_backup_limit: None,
            ludusavi_meta: None,
            device_bindings: HashMap::new(),
        }
    }

    #[test]
    fn scheduled_copy_is_replaced_by_the_stored_definition() {
        let stored = vec![game("Renamed", "key-1"), game("Other", "key-2")];

        let current = current_game(&stored, &game("Old name", "key-1")).unwrap();

        assert_eq!(current.name, "Renamed");
    }

    #[test]
    fn removed_game_is_not_backed_up_again() {
        let stored = vec![game("Other", "key-2")];

        assert!(current_game(&stored, &game("Removed", "key-1")).is_none());
    }
}
