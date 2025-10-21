use std::{
    collections::HashMap,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Context;
use log::{info, warn};
use rust_i18n::t;
use tauri::AppHandle;
use tokio::sync::{
    mpsc::{self, UnboundedReceiver, UnboundedSender},
    oneshot,
};
use tokio::time::{self, Sleep};
use tokio_util::sync::CancellationToken;

use crate::{
    backup::Game,
    config::{get_config, set_config},
};

use super::{QuickActionType, quick_apply, quick_backup};

const TIMER_TICK_SECONDS: u64 = 60;

pub enum QuickActionCommand {
    RegisterTrayItems {
        game_item: tauri::menu::MenuItem<tauri::Wry>,
        duration_items: HashMap<u32, tauri::menu::CheckMenuItem<tauri::Wry>>,
    },
    SetCurrentGame {
        game: Game,
        respond_to: oneshot::Sender<anyhow::Result<()>>,
    },
    UpdateInterval {
        minutes: u32,
    },
    TriggerBackup(QuickActionType),
    TriggerApply(QuickActionType),
}

#[derive(Default)]
struct QuickActionState {
    current_game: Option<Game>,
    auto_backup_minutes: u32,
    elapsed_minutes: u32,
    tray_game_item: Option<tauri::menu::MenuItem<tauri::Wry>>,
    tray_duration_items: HashMap<u32, tauri::menu::CheckMenuItem<tauri::Wry>>,
}

pub struct QuickActionManager {
    app: AppHandle,
    state: Mutex<QuickActionState>,
    command_tx: UnboundedSender<QuickActionCommand>,
    cancel_token: CancellationToken,
}

impl Drop for QuickActionManager {
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}

impl QuickActionManager {
    pub fn new(app: &AppHandle) -> Arc<Self> {
        let cancel_token = CancellationToken::new();
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let current_game = get_config()
            .ok()
            .and_then(|cfg| cfg.quick_action.quick_action_game.clone());

        let manager = Arc::new(Self {
            app: app.clone(),
            state: Mutex::new(QuickActionState {
                current_game,
                ..Default::default()
            }),
            command_tx,
            cancel_token: cancel_token.clone(),
        });

        QuickActionWorker::spawn(Arc::clone(&manager), command_rx, cancel_token);

        manager
    }

    pub async fn set_quick_backup_game(&self, game: Game) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.command_tx
            .send(QuickActionCommand::SetCurrentGame {
                game,
                respond_to: tx,
            })
            .context("failed to send SetCurrentGame command")?;
        rx.await
            .context("manager dropped SetCurrentGame response")??;
        Ok(())
    }

    pub fn update_interval(&self, minutes: u32) {
        if let Err(err) = self
            .command_tx
            .send(QuickActionCommand::UpdateInterval { minutes })
        {
            warn!(target: "rgsm::quick_action::manager", "Failed to send UpdateInterval command: {err}");
        }
    }

    pub fn trigger_backup(&self, trigger: QuickActionType) {
        if let Err(err) = self
            .command_tx
            .send(QuickActionCommand::TriggerBackup(trigger))
        {
            warn!(target: "rgsm::quick_action::manager", "Failed to send TriggerBackup command: {err}");
        }
    }

    pub fn trigger_apply(&self, trigger: QuickActionType) {
        if let Err(err) = self
            .command_tx
            .send(QuickActionCommand::TriggerApply(trigger))
        {
            warn!(target: "rgsm::quick_action::manager", "Failed to send TriggerApply command: {err}");
        }
    }

    pub fn register_tray_items(
        &self,
        game_item: tauri::menu::MenuItem<tauri::Wry>,
        duration_items: HashMap<u32, tauri::menu::CheckMenuItem<tauri::Wry>>,
    ) {
        if let Err(err) = self.command_tx.send(QuickActionCommand::RegisterTrayItems {
            game_item,
            duration_items,
        }) {
            warn!(target: "rgsm::quick_action::manager", "Failed to send RegisterTrayItems command: {err}");
        }
    }

    pub fn app_handle(&self) -> AppHandle {
        self.app.clone()
    }

    pub fn current_interval(&self) -> u32 {
        self.lock_state().auto_backup_minutes
    }

    pub fn current_game(&self) -> Option<Game> {
        self.lock_state().current_game.clone()
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, QuickActionState> {
        self.state
            .lock()
            .expect("QuickActionManager state poisoned")
    }
}

struct QuickActionWorker {
    manager: Arc<QuickActionManager>,
    command_rx: UnboundedReceiver<QuickActionCommand>,
    timer_sleep: Option<Pin<Box<Sleep>>>,
    cancel_token: CancellationToken,
}

impl QuickActionWorker {
    fn spawn(
        manager: Arc<QuickActionManager>,
        command_rx: UnboundedReceiver<QuickActionCommand>,
        cancel_token: CancellationToken,
    ) {
        let mut worker = Self {
            manager,
            command_rx,
            timer_sleep: None,
            cancel_token,
        };

        tauri::async_runtime::spawn(async move { worker.run().await });
    }

    async fn run(&mut self) {
        loop {
            if let Some(timer) = self.timer_sleep.as_mut() {
                tokio::select! {
                    _ = self.cancel_token.cancelled() => {
                        info!("QuickActionWorker received cancel signal, shutting down gracefully");
                        break;
                    },
                    _ = timer.as_mut() => {
                        self.handle_timer_tick().await;
                    }
                    cmd = self.command_rx.recv() => {
                        if let Some(cmd) = cmd {
                            self.handle_command(cmd).await;
                        } else {
                            break;
                        }
                    }
                }
            } else {
                tokio::select! {
                    _ = self.cancel_token.cancelled() => {
                        info!("QuickActionWorker received cancel signal, shutting down gracefully");
                        break;
                    },
                    cmd = self.command_rx.recv() => {
                        match cmd {
                            Some(cmd) => self.handle_command(cmd).await,
                            None => break,
                        }
                    }
                }
            }
        }
        info!(
            "QuickActionWorker received cancel signal or channel closed, shutting down gracefully"
        );
    }

    async fn handle_command(&mut self, command: QuickActionCommand) {
        match command {
            QuickActionCommand::RegisterTrayItems {
                game_item,
                duration_items,
            } => self.handle_register_tray(game_item, duration_items),
            QuickActionCommand::SetCurrentGame { game, respond_to } => {
                let result = self.handle_set_current_game(game).await;
                let _ = respond_to.send(result);
            }
            QuickActionCommand::UpdateInterval { minutes } => {
                self.handle_update_interval(minutes).await;
            }
            QuickActionCommand::TriggerBackup(trigger) => {
                let app = self.manager.app_handle();
                quick_backup(&app, trigger).await;
            }
            QuickActionCommand::TriggerApply(trigger) => {
                let app = self.manager.app_handle();
                quick_apply(&app, trigger).await;
            }
        }
    }

    fn handle_register_tray(
        &mut self,
        game_item: tauri::menu::MenuItem<tauri::Wry>,
        duration_items: HashMap<u32, tauri::menu::CheckMenuItem<tauri::Wry>>,
    ) {
        let mut state = self.manager.lock_state();
        state.tray_game_item = Some(game_item);
        state.tray_duration_items = duration_items;

        drop(state);
        self.refresh_tray_game_label();
        self.refresh_tray_duration_checks();
    }

    async fn handle_set_current_game(&mut self, game: Game) -> anyhow::Result<()> {
        let mut config = get_config().context("failed to load config")?;
        config.quick_action.quick_action_game = Some(game.clone());
        set_config(&config)
            .await
            .context("failed to persist quick action game")?;

        {
            let mut state = self.manager.lock_state();
            state.current_game = Some(game.clone());
        }

        self.manager
            .app_handle()
            .tray_by_id("tray_icon")
            .ok_or_else(|| anyhow::anyhow!("Cannot get tray"))?
            .set_title(Some(&game.name))?;

        self.refresh_tray_game_label();
        Ok(())
    }

    async fn handle_update_interval(&mut self, minutes: u32) {
        {
            let mut state = self.manager.lock_state();
            state.auto_backup_minutes = minutes;
            state.elapsed_minutes = 0;
        }
        self.refresh_tray_duration_checks();

        if minutes == 0 {
            self.timer_sleep = None;
            return;
        }

        self.timer_sleep = Some(Box::pin(time::sleep(Duration::from_secs(
            TIMER_TICK_SECONDS,
        ))));
    }

    async fn handle_timer_tick(&mut self) {
        let should_trigger = {
            let mut state = self.manager.lock_state();
            if state.auto_backup_minutes == 0 {
                self.timer_sleep = None;
                false
            } else {
                state.elapsed_minutes = state.elapsed_minutes.saturating_add(1);
                if state.elapsed_minutes >= state.auto_backup_minutes {
                    state.elapsed_minutes = 0;
                    true
                } else {
                    false
                }
            }
        };

        if should_trigger {
            let app = self.manager.app_handle();
            quick_backup(&app, QuickActionType::Timer).await;
        }

        if self.timer_sleep.is_some() {
            self.timer_sleep = Some(Box::pin(time::sleep(Duration::from_secs(
                TIMER_TICK_SECONDS,
            ))));
        }
    }

    fn refresh_tray_game_label(&self) {
        let (label, item) = {
            let state = self.manager.lock_state();
            let label = state
                .current_game
                .as_ref()
                .map(|game| game.name.clone())
                .unwrap_or_else(|| t!("backend.tray.no_game_selected").into());
            let item = state.tray_game_item.clone();
            (label, item)
        };

        if let Some(item) = item {
            if let Err(err) = item.set_text(label) {
                warn!(
                    target: "rgsm::quick_action::manager",
                    "Failed to refresh quick action game label: {err:?}"
                );
            }
        }
    }

    fn refresh_tray_duration_checks(&self) {
        let (current, items) = {
            let state = self.manager.lock_state();
            (state.auto_backup_minutes, state.tray_duration_items.clone())
        };

        for (duration, item) in items {
            if let Err(err) = item.set_checked(duration == current) {
                warn!(
                    target: "rgsm::quick_action::manager",
                    "Failed to refresh interval menu check for timer.{duration}: {err:?}"
                );
            }
        }
    }
}
