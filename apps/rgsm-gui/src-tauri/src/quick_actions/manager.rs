use std::sync::{Arc, Mutex};

use anyhow::Context;
use log::{info, warn};
use rust_i18n::t;
use tauri::AppHandle;
use tokio::sync::{
    mpsc::{self, UnboundedReceiver, UnboundedSender},
    oneshot,
};
use tokio_util::sync::CancellationToken;

use rgsm_core::{
    backup::Game,
    config::{get_config, set_config},
};

use super::{QuickActionType, quick_apply, quick_backup};

pub enum QuickActionCommand {
    RegisterTrayItems {
        game_item: tauri::menu::MenuItem<tauri::Wry>,
    },
    SetCurrentGame {
        game: Box<Game>,
        respond_to: oneshot::Sender<anyhow::Result<()>>,
    },
    TriggerBackup(QuickActionType),
    TriggerApply(QuickActionType),
}

#[derive(Default)]
struct QuickActionState {
    current_game: Option<Game>,
    tray_game_item: Option<tauri::menu::MenuItem<tauri::Wry>>,
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
                game: Box::new(game),
                respond_to: tx,
            })
            .context("failed to send SetCurrentGame command")?;
        rx.await
            .context("manager dropped SetCurrentGame response")??;
        Ok(())
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

    pub fn register_tray_items(&self, game_item: tauri::menu::MenuItem<tauri::Wry>) {
        if let Err(err) = self
            .command_tx
            .send(QuickActionCommand::RegisterTrayItems { game_item })
        {
            warn!(target: "rgsm::quick_action::manager", "Failed to send RegisterTrayItems command: {err}");
        }
    }

    pub fn app_handle(&self) -> AppHandle {
        self.app.clone()
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
            cancel_token,
        };

        tauri::async_runtime::spawn(async move { worker.run().await });
    }

    async fn run(&mut self) {
        loop {
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
        info!(
            "QuickActionWorker received cancel signal or channel closed, shutting down gracefully"
        );
    }

    async fn handle_command(&mut self, command: QuickActionCommand) {
        match command {
            QuickActionCommand::RegisterTrayItems { game_item } => {
                self.handle_register_tray(game_item)
            }
            QuickActionCommand::SetCurrentGame { game, respond_to } => {
                let result = self.handle_set_current_game(*game).await;
                let _ = respond_to.send(result);
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

    fn handle_register_tray(&mut self, game_item: tauri::menu::MenuItem<tauri::Wry>) {
        let mut state = self.manager.lock_state();
        state.tray_game_item = Some(game_item);
        drop(state);
        self.refresh_tray_game_label();
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
}
