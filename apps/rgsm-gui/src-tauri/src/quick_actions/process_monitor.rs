use std::collections::{HashMap, HashSet};
use std::time::Duration;

use log::{info, warn};
use tauri::AppHandle;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::time::Instant;

use rgsm_core::backup::{AutoBackupConfig, Game};
use rgsm_core::config::{GameAutomationSettings, get_config};
use rgsm_core::services::{LiveSaveSyncTarget, v2_live_save_sync_targets};

use crate::process_util::{process_is_running, process_name_for_game, running_process_names};

use super::{QuickActionType, perform_changed_auto_backup};

const PROCESS_POLL_INTERVAL: Duration = Duration::from_secs(2);

pub enum ProcessMonitorCommand {
    SyncFromConfig(Vec<MonitoredProcessGame>),
}

#[derive(Clone)]
pub struct MonitoredProcessGame {
    key: String,
    game: Game,
    automation: GameAutomationSettings,
    process_name: String,
}

struct ProcessRuntimeGame {
    game: Game,
    automation: GameAutomationSettings,
    process_name: String,
    was_running: bool,
    next_interval: Option<Instant>,
}

pub struct ProcessMonitor {
    command_tx: UnboundedSender<ProcessMonitorCommand>,
}

impl ProcessMonitor {
    pub fn spawn(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        tauri::async_runtime::spawn(process_monitor_loop(app, rx));
        Self { command_tx: tx }
    }

    pub fn sync_from_config(&self) {
        let entries = match get_config() {
            Ok(config) => monitored_process_games(&config),
            Err(err) => {
                warn!(
                    target: "rgsm::quick_action::process_monitor",
                    "Failed to load config for process monitor sync: {err:?}"
                );
                return;
            }
        };

        if let Err(err) = self
            .command_tx
            .send(ProcessMonitorCommand::SyncFromConfig(entries))
        {
            warn!(
                target: "rgsm::quick_action::process_monitor",
                "Failed to send process monitor sync command: {err}"
            );
        }
    }
}

fn monitored_process_games(config: &rgsm_core::config::Config) -> Vec<MonitoredProcessGame> {
    let mut entries = config
        .quick_action
        .game_automations
        .iter()
        .filter(|automation| automation.has_process_triggers())
        .filter_map(|automation| {
            let game = config
                .games
                .iter()
                .find(|game| automation.references_game(game))?
                .clone();
            let process_name = match process_name_for_game(&game, automation) {
                Some(process_name) => process_name,
                None => {
                    warn!(
                        target: "rgsm::quick_action::process_monitor",
                        "Skipping process automation without a process name: {}",
                        game.name
                    );
                    return None;
                }
            };
            Some((
                game_key(&game),
                MonitoredProcessGame {
                    key: game_key(&game),
                    game,
                    automation: automation.clone(),
                    process_name,
                },
            ))
        })
        .collect::<HashMap<_, _>>();

    match v2_live_save_sync_targets() {
        Ok(targets) => merge_live_save_exit_targets(&mut entries, config, targets),
        Err(err) => warn!(
            target: "rgsm::quick_action::process_monitor",
            "Failed to load Live Save Sync process targets: {err}"
        ),
    }
    entries.into_values().collect()
}

fn merge_live_save_exit_targets(
    entries: &mut HashMap<String, MonitoredProcessGame>,
    config: &rgsm_core::config::Config,
    targets: Vec<LiveSaveSyncTarget>,
) {
    for target in targets.into_iter().filter(|target| target.snapshot_on_exit) {
        let Some(game) = config
            .games
            .iter()
            .find(|game| game.storage_key == target.game_id)
            .cloned()
        else {
            continue;
        };
        let key = game_key(&game);
        if let Some(entry) = entries
            .get_mut(&key)
            .filter(|entry| entry.process_name == target.process_name)
        {
            entry.automation.on_process_exit = true;
            continue;
        }
        let key = if entries.contains_key(&key) {
            format!("{key}::live-save")
        } else {
            key
        };
        entries
            .entry(key.clone())
            .and_modify(|entry| {
                entry.automation.on_process_exit = true;
            })
            .or_insert_with(|| MonitoredProcessGame {
                key,
                automation: GameAutomationSettings {
                    storage_key: game.storage_key.clone(),
                    game_name: game.name.clone(),
                    process_name: target.process_name.clone(),
                    on_process_start: false,
                    on_process_exit: true,
                    in_process_interval_secs: None,
                },
                game,
                process_name: target.process_name,
            });
    }
}

async fn process_monitor_loop(app: AppHandle, mut rx: UnboundedReceiver<ProcessMonitorCommand>) {
    let mut games: HashMap<String, ProcessRuntimeGame> = HashMap::new();
    let mut interval = tokio::time::interval(PROCESS_POLL_INTERVAL);

    loop {
        tokio::select! {
            cmd = rx.recv() => {
                match cmd {
                    Some(ProcessMonitorCommand::SyncFromConfig(entries)) => {
                        sync_runtime_games(&mut games, entries);
                    }
                    None => break,
                }
            }
            _ = interval.tick() => {
                poll_runtime_games(&app, &mut games).await;
            }
        }
    }

    info!(
        target: "rgsm::quick_action::process_monitor",
        "Process monitor loop terminated"
    );
}

fn sync_runtime_games(
    games: &mut HashMap<String, ProcessRuntimeGame>,
    entries: Vec<MonitoredProcessGame>,
) {
    let incoming_keys: HashSet<String> = entries.iter().map(|entry| entry.key.clone()).collect();
    games.retain(|key, _| incoming_keys.contains(key));

    for entry in entries {
        if let Some(existing) = games.get_mut(&entry.key) {
            let previous_interval_secs = existing.automation.in_process_interval_secs;
            existing.game = entry.game;
            existing.automation = entry.automation;
            if existing.process_name != entry.process_name {
                existing.process_name = entry.process_name;
                existing.was_running = false;
                existing.next_interval = None;
            } else if existing.automation.in_process_interval_secs != previous_interval_secs {
                existing.next_interval = next_interval_at(
                    existing.automation.in_process_interval_secs,
                    existing.was_running,
                    Instant::now(),
                );
            }
        } else {
            info!(
                target: "rgsm::quick_action::process_monitor",
                "Enabling process monitor for '{}' ({})",
                entry.game.name, entry.process_name
            );
            games.insert(
                entry.key,
                ProcessRuntimeGame {
                    game: entry.game,
                    automation: entry.automation,
                    process_name: entry.process_name,
                    was_running: false,
                    next_interval: None,
                },
            );
        }
    }
}

async fn poll_runtime_games(app: &AppHandle, games: &mut HashMap<String, ProcessRuntimeGame>) {
    if games.is_empty() {
        return;
    }

    let processes = match running_process_names() {
        Ok(processes) => processes,
        Err(err) => {
            warn!(
                target: "rgsm::quick_action::process_monitor",
                "Failed to list running processes: {err:?}"
            );
            return;
        }
    };

    let now = Instant::now();
    for entry in games.values_mut() {
        let is_running = process_is_running(&processes, &entry.process_name);
        let mut triggers = Vec::new();

        if is_running && !entry.was_running {
            if entry.automation.on_process_start {
                triggers.push(QuickActionType::ProcessStart);
            }
            entry.next_interval = entry
                .automation
                .in_process_interval_secs
                .map(|secs| now + Duration::from_secs(secs as u64));
        } else if is_running
            && let Some(next_interval) = entry.next_interval
            && next_interval <= now
        {
            triggers.push(QuickActionType::ProcessInterval);
            entry.next_interval = entry
                .automation
                .in_process_interval_secs
                .map(|secs| now + Duration::from_secs(secs as u64));
        }

        if !is_running && entry.was_running {
            if entry.automation.on_process_exit {
                triggers.push(QuickActionType::ProcessExit);
            }
            entry.next_interval = None;
        }

        entry.was_running = is_running;

        for trigger in triggers {
            let retention = process_backup_retention(&entry.game);
            perform_changed_auto_backup(app, &entry.game, retention, trigger).await;
        }
    }
}

fn process_backup_retention(game: &Game) -> Option<&AutoBackupConfig> {
    game.auto_backup.as_ref()
}

fn game_key(game: &Game) -> String {
    if game.storage_key.is_empty() {
        game.name.clone()
    } else {
        game.storage_key.clone()
    }
}

fn next_interval_at(interval_secs: Option<u32>, is_running: bool, now: Instant) -> Option<Instant> {
    if !is_running {
        return None;
    }
    interval_secs.map(|secs| now + Duration::from_secs(secs as u64))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn sync_schedules_interval_when_enabled_for_running_process() {
        let mut games = runtime_games(Some(runtime_game(None, true, None)));

        sync_runtime_games(&mut games, vec![monitored_game(Some(60))]);

        assert!(games["game"].next_interval.is_some());
    }

    #[test]
    fn sync_clears_interval_when_disabled_for_running_process() {
        let mut games = runtime_games(Some(runtime_game(
            Some(30),
            true,
            Some(Instant::now() + Duration::from_secs(30)),
        )));

        sync_runtime_games(&mut games, vec![monitored_game(None)]);

        assert!(games["game"].next_interval.is_none());
    }

    #[test]
    fn sync_preserves_existing_interval_when_interval_is_unchanged() {
        let original_next_interval = Instant::now() + Duration::from_secs(30);
        let mut games = runtime_games(Some(runtime_game(
            Some(30),
            true,
            Some(original_next_interval),
        )));

        sync_runtime_games(&mut games, vec![monitored_game(Some(30))]);

        assert_eq!(games["game"].next_interval, Some(original_next_interval));
    }

    #[test]
    fn process_backup_retention_reuses_game_auto_backup_config() {
        let mut game = test_game();
        game.auto_backup = Some(AutoBackupConfig {
            interval_secs: 300,
            max_backup_count: Some(12),
        });

        assert_eq!(
            process_backup_retention(&game).and_then(|retention| retention.max_backup_count),
            Some(12)
        );
    }

    #[test]
    fn process_backup_retention_is_absent_without_game_auto_backup_config() {
        assert!(process_backup_retention(&test_game()).is_none());
    }

    #[test]
    fn live_save_exit_target_adds_process_exit_monitoring() {
        let mut config = rgsm_core::config::Config::default();
        config.games.push(test_game());
        let mut entries = HashMap::new();

        merge_live_save_exit_targets(
            &mut entries,
            &config,
            vec![live_save_target("live-game.exe", true)],
        );

        let entry = &entries["game"];
        assert_eq!(entry.process_name, "live-game.exe");
        assert!(entry.automation.on_process_exit);
        assert!(!entry.automation.on_process_start);
        assert_eq!(entry.automation.in_process_interval_secs, None);
    }

    #[test]
    fn live_save_exit_target_does_not_retarget_other_process_triggers() {
        let mut config = rgsm_core::config::Config::default();
        config.games.push(test_game());
        let mut existing = monitored_game(Some(60));
        existing.automation.on_process_start = true;
        let mut entries = HashMap::from([("game".to_string(), existing)]);

        merge_live_save_exit_targets(
            &mut entries,
            &config,
            vec![live_save_target("live-game.exe", true)],
        );

        let existing = &entries["game"];
        assert_eq!(existing.process_name, "game.exe");
        assert!(existing.automation.on_process_start);
        assert!(!existing.automation.on_process_exit);
        assert_eq!(existing.automation.in_process_interval_secs, Some(60));
        let live_save = &entries["game::live-save"];
        assert_eq!(live_save.process_name, "live-game.exe");
        assert!(live_save.automation.on_process_exit);
    }

    #[test]
    fn disabled_live_save_exit_target_adds_no_monitor() {
        let mut config = rgsm_core::config::Config::default();
        config.games.push(test_game());
        let mut entries = HashMap::new();

        merge_live_save_exit_targets(
            &mut entries,
            &config,
            vec![live_save_target("live-game.exe", false)],
        );

        assert!(entries.is_empty());
    }

    fn runtime_games(
        runtime_game: Option<ProcessRuntimeGame>,
    ) -> HashMap<String, ProcessRuntimeGame> {
        let mut games = HashMap::new();
        if let Some(runtime_game) = runtime_game {
            games.insert("game".to_string(), runtime_game);
        }
        games
    }

    fn runtime_game(
        interval_secs: Option<u32>,
        was_running: bool,
        next_interval: Option<Instant>,
    ) -> ProcessRuntimeGame {
        ProcessRuntimeGame {
            game: test_game(),
            automation: automation(interval_secs),
            process_name: "game.exe".to_string(),
            was_running,
            next_interval,
        }
    }

    fn monitored_game(interval_secs: Option<u32>) -> MonitoredProcessGame {
        MonitoredProcessGame {
            key: "game".to_string(),
            game: test_game(),
            automation: automation(interval_secs),
            process_name: "game.exe".to_string(),
        }
    }

    fn automation(interval_secs: Option<u32>) -> GameAutomationSettings {
        GameAutomationSettings {
            storage_key: "game".to_string(),
            game_name: "Game".to_string(),
            process_name: "game.exe".to_string(),
            on_process_start: false,
            on_process_exit: false,
            in_process_interval_secs: interval_secs,
        }
    }

    fn live_save_target(process_name: &str, snapshot_on_exit: bool) -> LiveSaveSyncTarget {
        LiveSaveSyncTarget {
            game_id: "game".to_string(),
            process_name: process_name.to_string(),
            snapshot_on_exit,
        }
    }

    fn test_game() -> Game {
        Game {
            name: "Game".to_string(),
            storage_key: "game".to_string(),
            save_paths: Vec::new(),
            game_paths: HashMap::new(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            ludusavi_meta: None,
            device_bindings: HashMap::new(),
        }
    }
}
