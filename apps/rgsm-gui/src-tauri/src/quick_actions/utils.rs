use crate::sound::{QuickActionSoundEffect, play_quick_action_sound};
use log::{error, info, warn};
use rgsm_core::{
    backup::{CreatedBy, Game, TIMER_AUTO_BACKUP_DESCRIPTION},
    config::{QuickActionSoundPreferences, QuickActionsSettings, get_config},
    hooks::HookSource,
    preclude::*,
    services::ServiceContext,
};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub enum QuickActionType {
    Timer,
    Tray,
    Hotkey,
    ProcessStart,
    ProcessExit,
    ProcessInterval,
}

impl QuickActionType {
    pub(super) fn generate_describe(self) -> String {
        match self {
            QuickActionType::Timer => String::from(TIMER_AUTO_BACKUP_DESCRIPTION),
            QuickActionType::Tray => String::from("Quick Backup (Tray)"),
            QuickActionType::Hotkey => String::from("Quick Backup (Hotkey)"),
            QuickActionType::ProcessStart => String::from("Auto Backup (Process Start)"),
            QuickActionType::ProcessExit => String::from("Auto Backup (Process Exit)"),
            QuickActionType::ProcessInterval => String::from("Auto Backup (Process Interval)"),
        }
    }

    /// Convert to the corresponding HookSource variant.
    pub fn to_hook_source(self) -> HookSource {
        match self {
            QuickActionType::Timer => HookSource::TimerAutoBackup,
            QuickActionType::Tray => HookSource::QuickActionTray,
            QuickActionType::Hotkey => HookSource::QuickActionHotkey,
            QuickActionType::ProcessStart
            | QuickActionType::ProcessExit
            | QuickActionType::ProcessInterval => HookSource::ProcessMonitorAutoBackup,
        }
    }

    /// Convert to the corresponding CreatedBy variant for snapshot metadata.
    pub fn to_created_by(self) -> CreatedBy {
        match self {
            QuickActionType::Timer => CreatedBy::Timer,
            QuickActionType::Tray => CreatedBy::Tray,
            QuickActionType::Hotkey => CreatedBy::Hotkey,
            QuickActionType::ProcessStart => CreatedBy::ProcessStart,
            QuickActionType::ProcessExit => CreatedBy::ProcessExit,
            QuickActionType::ProcessInterval => CreatedBy::ProcessInterval,
        }
    }

    pub fn is_auto_backup(self) -> bool {
        matches!(
            self,
            QuickActionType::Timer
                | QuickActionType::ProcessStart
                | QuickActionType::ProcessExit
                | QuickActionType::ProcessInterval
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub enum QuickActionOperation {
    Backup,
    Apply,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub enum QuickActionStatus {
    Success,
    Failure,
    SkippedUnchanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct QuickActionCompleted {
    pub operation: QuickActionOperation,
    pub status: QuickActionStatus,
    pub trigger: QuickActionType,
    /// Storage key of the game; display names are not unique.
    pub game_id: Option<String>,
    pub game_name: Option<String>,
}

pub fn emit_quick_action_event(
    app: &AppHandle,
    trigger: QuickActionType,
    operation: QuickActionOperation,
    status: QuickActionStatus,
    game: &Game,
) {
    let event = QuickActionCompleted {
        operation,
        status,
        trigger,
        game_id: Some(game.storage_key.clone()),
        game_name: Some(game.name.clone()),
    };
    crate::http::emit(app, "quick-action-completed", &event);
}

pub fn notify_backup_skipped_unchanged(
    app: &AppHandle,
    config: &rgsm_core::config::Config,
    trigger: QuickActionType,
    game: &Game,
) {
    let quick_settings = &config.quick_action;
    let sound_preferences = QuickActionSoundPreferences::from(quick_settings);
    let should_feedback = quick_settings.notify_when_unchanged
        && should_show_auto_backup_notification(config, trigger);
    if quick_settings.enable_notification && should_feedback {
        show_notification(
            t!("backend.tray.unchanged"),
            t!("backend.tray.unchanged_detail", game = game.name),
        );
    }
    if should_feedback {
        play_quick_action_sound(app, sound_preferences, QuickActionSoundEffect::Success);
    }
    emit_quick_action_event(
        app,
        trigger,
        QuickActionOperation::Backup,
        QuickActionStatus::SkippedUnchanged,
        game,
    );
}

pub fn notify_quick_action_failed(
    app: &AppHandle,
    config: &rgsm_core::config::Config,
    trigger: QuickActionType,
    operation: QuickActionOperation,
    game: &Game,
    error: &str,
) {
    let quick_settings = &config.quick_action;
    let sound_preferences = QuickActionSoundPreferences::from(quick_settings);
    if quick_settings.enable_notification {
        let detail = match operation {
            QuickActionOperation::Backup => {
                t!("backend.tray.backup_failed_detail", game = game.name)
            }
            QuickActionOperation::Apply => t!("backend.tray.apply_failed_detail", game = game.name),
        };
        show_notification(t!("backend.tray.error"), format!("{detail}\n{error}"));
    }
    play_quick_action_sound(app, sound_preferences, QuickActionSoundEffect::Failure);
    emit_quick_action_event(app, trigger, operation, QuickActionStatus::Failure, game);
}

/// Hotkey and tray presses are explicit requests, so a refusal must be visible.
/// Automatic triggers only log: a notification on every tick would be noise.
pub fn notify_paused_for_update(app: &AppHandle, trigger: QuickActionType) {
    warn!(
        target: "rgsm::quick_action",
        "Skipped {:?} quick action: an application update is being installed",
        trigger
    );
    if trigger.is_auto_backup() {
        return;
    }
    let Ok(config) = get_config() else {
        return;
    };
    let quick_settings = &config.quick_action;
    maybe_show_notification(
        quick_settings,
        t!("backend.tray.error"),
        t!("backend.tray.paused_for_update"),
    );
    play_quick_action_sound(
        app,
        QuickActionSoundPreferences::from(quick_settings),
        QuickActionSoundEffect::Failure,
    );
}

pub async fn quick_apply(app: &AppHandle, t: QuickActionType) {
    let Some(_operation) = app.state::<crate::app_operations::AppOperations>().begin() else {
        notify_paused_for_update(app, t);
        return;
    };
    info!(target:"rgsm::quick_action", "Auto apply triggered: {:#?}", t.generate_describe());
    let config = match get_config() {
        Ok(config) => config,
        Err(err) => {
            error!(target:"rgsm::quick_action", "Failed to load config: {err:?}");
            return;
        }
    };

    let quick_settings = config.quick_action.clone();
    let sound_preferences: QuickActionSoundPreferences =
        QuickActionSoundPreferences::from(&quick_settings);

    // 检查游戏是否已选择
    let game = match quick_settings.selected_game(&config.games).cloned() {
        Some(game) => game,
        None => {
            show_no_game_selected_error(app, &quick_settings, &sound_preferences);
            return;
        }
    };

    info!(target:"rgsm::quick_action", "Quick apply game: {:#?}", game);

    // 执行恢复操作
    let result = ServiceContext::new(app.state::<crate::hooks::HookPipelineState>().snapshot())
        .quick_apply(&game, t.to_hook_source(), None)
        .await;

    // 处理结果
    if let Err(e) = result {
        error!(target:"rgsm::quick_action", "Quick apply failed: {e:?}");
        notify_quick_action_failed(
            app,
            &config,
            t,
            QuickActionOperation::Apply,
            &game,
            &e.to_string(),
        );
    }
}

pub async fn quick_backup(app: &AppHandle, t: QuickActionType) {
    let Some(_operation) = app.state::<crate::app_operations::AppOperations>().begin() else {
        notify_paused_for_update(app, t);
        return;
    };
    info!(target:"rgsm::quick_action", "Auto backup triggered: {:#?}", t.generate_describe());
    let config = match get_config() {
        Ok(config) => config,
        Err(err) => {
            error!(target:"rgsm::quick_action", "Failed to load config: {err:?}");
            return;
        }
    };

    let quick_settings = config.quick_action.clone();
    let sound_preferences: QuickActionSoundPreferences =
        QuickActionSoundPreferences::from(&quick_settings);

    // 检查游戏是否已选择
    let game = match quick_settings.selected_game(&config.games).cloned() {
        Some(game) => game,
        None => {
            show_no_game_selected_error(app, &quick_settings, &sound_preferences);
            return;
        }
    };

    // 执行备份操作
    let result = ServiceContext::new(app.state::<crate::hooks::HookPipelineState>().snapshot())
        .quick_backup(
            &game,
            &t.generate_describe(),
            t.to_created_by(),
            t.to_hook_source(),
        )
        .await;

    if let Err(e) = result {
        error!(target:"rgsm::quick_action", "Quick backup failed: {e:?}");
        notify_quick_action_failed(
            app,
            &config,
            t,
            QuickActionOperation::Backup,
            &game,
            &e.to_string(),
        );
    }
}

fn show_no_game_selected_error(
    app: &AppHandle,
    settings: &QuickActionsSettings,
    sound_preferences: &QuickActionSoundPreferences,
) {
    warn!(target:"rgsm::quick_action", "No game selected, cannot quick backup/apply");
    maybe_show_notification(
        settings,
        t!("backend.tray.error"),
        t!("backend.tray.no_game_selected"),
    );
    play_quick_action_sound(
        app,
        sound_preferences.clone(),
        QuickActionSoundEffect::Failure,
    );
}

fn maybe_show_notification<T1: AsRef<str>, T2: AsRef<str>>(
    settings: &QuickActionsSettings,
    title: T1,
    body: T2,
) {
    if settings.enable_notification {
        show_notification(title, body);
    }
}

pub fn should_show_auto_backup_notification(
    config: &rgsm_core::config::Config,
    trigger: QuickActionType,
) -> bool {
    !trigger.is_auto_backup() || config.settings.prompt_when_auto_backup
}
