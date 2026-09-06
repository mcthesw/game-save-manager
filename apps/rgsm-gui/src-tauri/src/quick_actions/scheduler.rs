use std::collections::HashMap;
use std::time::Duration;

use log::{info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use tokio::time::Instant;

use rgsm_core::backup::{AutoBackupConfig, Game};
use rgsm_core::config::get_config;

use super::{QuickActionType, perform_changed_auto_backup};

/// Commands sent to the scheduler's event loop.
pub enum SchedulerCommand {
    /// Bulk-sync scheduler state from persisted game configs.
    /// Called on startup and whenever game configs change.
    SyncFromConfig(Vec<Game>),
    /// Query current scheduler status.
    GetStatus {
        respond_to: oneshot::Sender<Vec<AutoBackupGameStatus>>,
    },
}

/// Status of one game's auto-backup timer, returned by `GetStatus`.
#[derive(Debug, Clone, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct AutoBackupGameStatus {
    pub game_id: String,
    pub game_name: String,
    pub interval_secs: u32,
}

/// Internal state for a game with an active auto-backup timer.
struct ScheduledGame {
    game: Game,
    config: AutoBackupConfig,
    next_trigger: Instant,
}

/// Handle for sending commands to the auto-backup scheduler.
pub struct AutoBackupScheduler {
    command_tx: UnboundedSender<SchedulerCommand>,
}

impl AutoBackupScheduler {
    /// Spawn the scheduler task and return a handle.
    pub fn spawn(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        tauri::async_runtime::spawn(scheduler_loop(app, rx));
        Self { command_tx: tx }
    }

    pub fn send(&self, cmd: SchedulerCommand) {
        if let Err(err) = self.command_tx.send(cmd) {
            warn!(
                target: "rgsm::scheduler",
                "Failed to send scheduler command: {err}"
            );
        }
    }

    /// Sync scheduler state from config — reads all games and enables timers for those with auto_backup.
    pub fn sync_from_config(&self) {
        let entries = match get_config() {
            Ok(config) => config.games,
            Err(e) => {
                warn!(
                    target: "rgsm::scheduler",
                    "Failed to load config for scheduler sync: {e:?}"
                );
                return;
            }
        };
        self.send(SchedulerCommand::SyncFromConfig(entries));
    }

    pub async fn get_status(&self) -> Vec<AutoBackupGameStatus> {
        let (tx, rx) = oneshot::channel();
        self.send(SchedulerCommand::GetStatus { respond_to: tx });
        rx.await.unwrap_or_default()
    }
}

/// The scheduler's event loop.
async fn scheduler_loop(app: AppHandle, mut rx: UnboundedReceiver<SchedulerCommand>) {
    let mut games: HashMap<String, ScheduledGame> = HashMap::new();

    loop {
        let deadline = games
            .values()
            .map(|g| g.next_trigger)
            .min()
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(86400 * 365));

        tokio::select! {
            biased;
            cmd = rx.recv() => {
                match cmd {
                    Some(cmd) => handle_command(&mut games, cmd),
                    None => break,
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                trigger_due_games(&app, &mut games).await;
            }
        }
    }

    info!(
        target: "rgsm::scheduler",
        "Auto-backup scheduler loop terminated"
    );
}

fn handle_command(games: &mut HashMap<String, ScheduledGame>, cmd: SchedulerCommand) {
    match cmd {
        SchedulerCommand::SyncFromConfig(entries) => {
            let now = Instant::now();
            let mut previous = std::mem::take(games);
            for game in entries {
                let Some(config) = game.auto_backup.clone() else {
                    continue;
                };
                if game.storage_key.is_empty() || config.interval_secs == 0 {
                    warn!(
                        target: "rgsm::scheduler",
                        "Ignoring invalid auto-backup settings for '{}'",
                        game.name
                    );
                    continue;
                }
                // Renaming or editing a game must not postpone its next backup.
                let next_trigger = previous
                    .remove(&game.storage_key)
                    .filter(|scheduled| scheduled.config.interval_secs == config.interval_secs)
                    .map(|scheduled| scheduled.next_trigger)
                    .unwrap_or_else(|| now + Duration::from_secs(config.interval_secs as u64));
                games.insert(
                    game.storage_key.clone(),
                    ScheduledGame {
                        game,
                        config,
                        next_trigger,
                    },
                );
            }
        }
        SchedulerCommand::GetStatus { respond_to } => {
            let status: Vec<AutoBackupGameStatus> = games
                .values()
                .map(|sg| AutoBackupGameStatus {
                    game_id: sg.game.storage_key.clone(),
                    game_name: sg.game.name.clone(),
                    interval_secs: sg.config.interval_secs,
                })
                .collect();
            let _ = respond_to.send(status);
        }
    }
}

/// Trigger auto-backups for all games whose timers have fired.
async fn trigger_due_games(app: &AppHandle, games: &mut HashMap<String, ScheduledGame>) {
    let now = Instant::now();
    let due_ids: Vec<String> = games
        .iter()
        .filter(|(_, sg)| sg.next_trigger <= now)
        .map(|(id, _)| id.clone())
        .collect();

    for id in due_ids {
        if let Some(sg) = games.get_mut(&id) {
            perform_timer_backup(app, &sg.game, &sg.config).await;
            sg.next_trigger = Instant::now() + Duration::from_secs(sg.config.interval_secs as u64);
        }
    }
}

/// Perform a timer auto-backup for a specific game, including hooks and cleanup.
async fn perform_timer_backup(app: &AppHandle, game: &Game, backup_config: &AutoBackupConfig) {
    perform_changed_auto_backup(app, game, Some(backup_config), QuickActionType::Timer).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn auto_backup_config(interval_secs: u32) -> AutoBackupConfig {
        AutoBackupConfig {
            interval_secs,
            max_backup_count: None,
        }
    }

    fn test_game(name: &str, launch_path: &str, auto_backup: Option<AutoBackupConfig>) -> Game {
        let mut game_paths = HashMap::new();
        game_paths.insert("device-1".to_string(), launch_path.to_string());
        Game {
            name: name.to_string(),
            storage_key: name.to_string(),
            save_paths: vec![],
            game_paths,
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn same_title_games_keep_independent_timers() {
        let interval = auto_backup_config(30);
        let mut first = test_game("Same", "first.exe", Some(interval.clone()));
        first.storage_key = "first".into();
        let mut second = test_game("Same", "second.exe", Some(interval.clone()));
        second.storage_key = "second".into();
        let mut games = HashMap::new();

        handle_command(
            &mut games,
            SchedulerCommand::SyncFromConfig(vec![first, second]),
        );

        assert_eq!(games.len(), 2);
        assert_eq!(games["first"].game.name, "Same");
        assert_eq!(games["second"].game.name, "Same");
    }

    #[test]
    fn sync_from_config_updates_existing_game_without_resetting_deadline_when_interval_matches() {
        let interval = auto_backup_config(30);
        let original_game = test_game("GameA", "C:\\old.exe", Some(interval.clone()));
        let mut updated_game = test_game("GameA", "C:\\new.exe", Some(interval.clone()));
        updated_game.name = "Renamed".into();
        let original_deadline = Instant::now() + Duration::from_secs(123);

        let mut games = HashMap::new();
        games.insert(
            "GameA".to_string(),
            ScheduledGame {
                game: original_game,
                config: interval.clone(),
                next_trigger: original_deadline,
            },
        );

        handle_command(
            &mut games,
            SchedulerCommand::SyncFromConfig(vec![updated_game.clone()]),
        );

        let synced = games.get("GameA").expect("game should still be scheduled");
        assert_eq!(synced.game.game_paths, updated_game.game_paths);
        assert_eq!(synced.game.name, "Renamed");
        assert_eq!(synced.next_trigger, original_deadline);
    }

    #[test]
    fn sync_from_config_removes_games_that_are_no_longer_enabled() {
        let game_a = test_game("GameA", "C:\\game-a.exe", Some(auto_backup_config(30)));
        let game_b = test_game("GameB", "C:\\game-b.exe", Some(auto_backup_config(45)));

        let mut games = HashMap::new();
        games.insert(
            "GameA".to_string(),
            ScheduledGame {
                game: game_a.clone(),
                config: auto_backup_config(30),
                next_trigger: Instant::now() + Duration::from_secs(30),
            },
        );
        games.insert(
            "GameB".to_string(),
            ScheduledGame {
                game: game_b,
                config: auto_backup_config(45),
                next_trigger: Instant::now() + Duration::from_secs(45),
            },
        );

        handle_command(&mut games, SchedulerCommand::SyncFromConfig(vec![game_a]));

        assert!(games.contains_key("GameA"));
        assert!(!games.contains_key("GameB"));
    }

    #[test]
    fn disabled_or_invalid_timers_are_not_scheduled() {
        let enabled = test_game("GameA", "game.exe", Some(auto_backup_config(30)));
        let mut games = HashMap::new();
        handle_command(
            &mut games,
            SchedulerCommand::SyncFromConfig(vec![enabled.clone()]),
        );
        assert_eq!(games.len(), 1);
        let mut disabled = enabled;
        disabled.auto_backup = None;
        let zero = test_game("Zero", "game.exe", Some(auto_backup_config(0)));
        let no_id = test_game("", "game.exe", Some(auto_backup_config(30)));
        handle_command(
            &mut games,
            SchedulerCommand::SyncFromConfig(vec![disabled, zero, no_id]),
        );
        assert!(games.is_empty());
    }

    #[test]
    fn changed_interval_resets_the_deadline() {
        let mut games = HashMap::new();
        let game = test_game("GameA", "game.exe", Some(auto_backup_config(30)));
        handle_command(
            &mut games,
            SchedulerCommand::SyncFromConfig(vec![game.clone()]),
        );
        let mut updated = game;
        updated.auto_backup = Some(auto_backup_config(60));
        let before = Instant::now();
        handle_command(&mut games, SchedulerCommand::SyncFromConfig(vec![updated]));
        assert!(games["GameA"].next_trigger >= before + Duration::from_secs(60));
        assert!(games["GameA"].next_trigger <= Instant::now() + Duration::from_secs(60));
    }
}
