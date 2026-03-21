use std::collections::HashMap;
use std::time::Duration;

use log::{info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use tokio::time::Instant;

use std::path::PathBuf;

use crate::backup::{AutoBackupConfig, Game, TimerSnapshotDecision};
use crate::config::get_config;
use crate::hooks::{HookSource, SnapshotCreatedCtx, SnapshotDeletedCtx};

use super::QuickActionType;

/// Commands sent to the scheduler's event loop.
pub enum SchedulerCommand {
    /// Bulk-sync scheduler state from persisted game configs.
    /// Called on startup and whenever game configs change.
    SyncFromConfig(Vec<(String, Game, AutoBackupConfig)>),
    /// Query current scheduler status.
    GetStatus {
        respond_to: oneshot::Sender<Vec<AutoBackupGameStatus>>,
    },
}

/// Status of one game's auto-backup timer, returned by `GetStatus`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AutoBackupGameStatus {
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
            Ok(config) => config
                .games
                .into_iter()
                .filter_map(|game| {
                    let config = game.auto_backup.clone()?;
                    Some((game.name.clone(), game, config))
                })
                .collect(),
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
            let new_names: std::collections::HashSet<String> =
                entries.iter().map(|(name, _, _)| name.clone()).collect();

            // Remove games no longer in config
            games.retain(|name, _| new_names.contains(name));

            for (name, game, config) in entries {
                if let Some(existing) = games.get_mut(&name) {
                    // Update game reference and config; keep existing schedule
                    // if interval hasn't changed, otherwise reset
                    if existing.config.interval_secs != config.interval_secs {
                        existing.next_trigger =
                            now + Duration::from_secs(config.interval_secs as u64);
                    }
                    existing.game = game;
                    existing.config = config;
                } else {
                    // New game — schedule first trigger
                    info!(
                        target: "rgsm::scheduler",
                        "Enabling auto-backup for '{}' every {}s",
                        name,
                        config.interval_secs
                    );
                    games.insert(
                        name,
                        ScheduledGame {
                            game,
                            next_trigger: now + Duration::from_secs(config.interval_secs as u64),
                            config,
                        },
                    );
                }
            }
        }
        SchedulerCommand::GetStatus { respond_to } => {
            let status: Vec<AutoBackupGameStatus> = games
                .iter()
                .map(|(name, sg)| AutoBackupGameStatus {
                    game_name: name.clone(),
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
    let due_names: Vec<String> = games
        .iter()
        .filter(|(_, sg)| sg.next_trigger <= now)
        .map(|(name, _)| name.clone())
        .collect();

    for name in due_names {
        if let Some(sg) = games.get_mut(&name) {
            perform_timer_backup(app, &sg.game, &sg.config).await;
            sg.next_trigger = Instant::now() + Duration::from_secs(sg.config.interval_secs as u64);
        }
    }
}

/// Perform a timer auto-backup for a specific game, including hooks and cleanup.
async fn perform_timer_backup(app: &AppHandle, game: &Game, backup_config: &AutoBackupConfig) {
    let describe = QuickActionType::Timer.generate_describe();

    match game.create_timer_snapshot_if_changed(&describe).await {
        Ok(TimerSnapshotDecision::SkippedUnchanged) => {
            info!(
                target: "rgsm::scheduler",
                "Skipped auto-backup for '{}': state unchanged",
                game.name
            );
            return;
        }
        Ok(TimerSnapshotDecision::Created) => {
            info!(
                target: "rgsm::scheduler",
                "Auto-backup created for '{}'",
                game.name
            );
        }
        Err(e) => {
            warn!(
                target: "rgsm::scheduler",
                "Auto-backup failed for '{}': {e:?}",
                game.name
            );
            return;
        }
    }

    let config = match get_config() {
        Ok(c) => c,
        Err(e) => {
            warn!(
                target: "rgsm::scheduler",
                "Failed to load config after auto-backup: {e:?}"
            );
            return;
        }
    };

    match game.get_game_snapshots_info() {
        Ok(snapshots) => {
            if let Some(snapshot) = snapshots.backups.last().cloned() {
                let local_zip_path = PathBuf::from(&snapshot.path);
                let remote_zip_path = format!("save_data/{}/{}.zip", snapshots.name, snapshot.date);
                let pipeline = app.state::<crate::hooks::HookPipelineState>().snapshot();
                let mut ctx = SnapshotCreatedCtx {
                    config: config.clone(),
                    source: HookSource::TimerAutoBackup,
                    game: game.clone(),
                    snapshot,
                    snapshots,
                    local_zip_path,
                    remote_zip_path,
                };
                pipeline.fire_snapshot_created(&mut ctx).await;
                if let Err(err) = game.set_game_snapshots_info(&ctx.snapshots) {
                    warn!(
                        target: "rgsm::scheduler",
                        "Failed to persist hook-updated snapshot metadata for '{}': {err:?}",
                        game.name
                    );
                }
            }
        }
        Err(err) => {
            warn!(
                target: "rgsm::scheduler",
                "Failed to read snapshots after auto-backup for '{}': {err:?}",
                game.name
            );
        }
    }

    // Cleanup old auto-backups
    let effective_max = backup_config
        .max_backup_count
        .unwrap_or(config.settings.max_auto_backup_count);

    if effective_max > 0 {
        match game.cleanup_old_auto_backups(effective_max).await {
            Ok(cleanup_result) => {
                if !cleanup_result.deleted_remote_paths.is_empty() {
                    let pipeline = app.state::<crate::hooks::HookPipelineState>().snapshot();
                    pipeline
                        .fire_snapshot_deleted(&SnapshotDeletedCtx {
                            config,
                            source: HookSource::TimerAutoBackup,
                            game: game.clone(),
                            snapshots: cleanup_result.snapshots,
                            deleted_remote_paths: cleanup_result.deleted_remote_paths,
                        })
                        .await;
                }
            }
            Err(e) => {
                warn!(
                    target: "rgsm::scheduler",
                    "Auto-backup cleanup failed for '{}': {e:?}",
                    game.name
                );
            }
        }
    }
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
            save_paths: vec![],
            game_paths,
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup,
        }
    }

    #[test]
    fn sync_from_config_updates_existing_game_without_resetting_deadline_when_interval_matches() {
        let interval = auto_backup_config(30);
        let original_game = test_game("GameA", "C:\\old.exe", Some(interval.clone()));
        let updated_game = test_game("GameA", "C:\\new.exe", Some(interval.clone()));
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
            SchedulerCommand::SyncFromConfig(vec![(
                "GameA".to_string(),
                updated_game.clone(),
                interval,
            )]),
        );

        let synced = games.get("GameA").expect("game should still be scheduled");
        assert_eq!(synced.game.game_paths, updated_game.game_paths);
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

        handle_command(
            &mut games,
            SchedulerCommand::SyncFromConfig(vec![(
                "GameA".to_string(),
                game_a,
                auto_backup_config(30),
            )]),
        );

        assert!(games.contains_key("GameA"));
        assert!(!games.contains_key("GameB"));
    }
}
