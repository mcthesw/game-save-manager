use crate::{
    config::{QuickActionSoundPreferences, QuickActionsSettings, get_config},
    preclude::*,
    sound::{QuickActionSoundEffect, play_quick_action_sound},
};
use log::{error, info, warn};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub enum QuickActionType {
    Timer,
    Tray,
    Hotkey,
}

impl QuickActionType {
    fn generate_describe(self) -> String {
        match self {
            QuickActionType::Timer => String::from("Auto Backup (Timer)"),
            QuickActionType::Tray => String::from("Quick Backup (Tray)"),
            QuickActionType::Hotkey => String::from("Quick Backup (Hotkey)"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
pub enum QuickActionOperation {
    Backup,
    Apply,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
pub enum QuickActionStatus {
    Success,
    Failure,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct QuickActionCompleted {
    pub operation: QuickActionOperation,
    pub status: QuickActionStatus,
    pub trigger: QuickActionType,
    pub game_name: Option<String>,
}

fn emit_quick_action_event(
    app: &AppHandle,
    trigger: QuickActionType,
    operation: QuickActionOperation,
    status: QuickActionStatus,
    game_name: Option<String>,
) {
    if let Err(err) = (QuickActionCompleted {
        operation,
        status,
        trigger,
        game_name,
    })
    .emit(app)
    {
        warn!(
            target: "rgsm::quick_action",
            "Failed to emit quick action event: {err:?}"
        );
    }
}

pub async fn quick_apply(app: &AppHandle, t: QuickActionType) {
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
    let game = match quick_settings.quick_action_game.clone() {
        Some(game) => game,
        None => {
            emit_quick_action_event(
                app,
                t,
                QuickActionOperation::Apply,
                QuickActionStatus::Failure,
                None,
            );
            show_no_game_selected_error(app, &quick_settings, &sound_preferences);
            return;
        }
    };

    info!(target:"rgsm::quick_action", "Quick apply game: {:#?}", game);

    // 执行恢复操作
    let result = async {
        let newest_date = game
            .get_game_snapshots_info()?
            .backups
            .last()
            .ok_or(BackupError::NoBackupAvailable)?
            .date
            .clone();
        game.restore_snapshot(&newest_date, None)
    }
    .await;

    // 处理结果
    match result {
        Err(e) => {
            error!(target:"rgsm::quick_action", "Quick apply failed: {:#?}", &e);
            maybe_show_notification(
                &quick_settings,
                t!("backend.tray.error"),
                format!("{:#?}\n{:#?}", t!("backend.tray.find_error_detail"), e),
            );
            play_quick_action_sound(app, sound_preferences, QuickActionSoundEffect::Failure);
            emit_quick_action_event(
                app,
                t,
                QuickActionOperation::Apply,
                QuickActionStatus::Failure,
                Some(game.name.clone()),
            );
        }
        Ok(_) => {
            maybe_show_success_notification(
                &quick_settings,
                true,
                t!("backend.tray.success"),
                format!(
                    "{:#?} {} {}",
                    game.name,
                    t!("backend.tray.quick_apply"),
                    t!("backend.tray.success")
                ),
            );
            play_quick_action_sound(app, sound_preferences, QuickActionSoundEffect::Success);
            emit_quick_action_event(
                app,
                t,
                QuickActionOperation::Apply,
                QuickActionStatus::Success,
                Some(game.name.clone()),
            );
        }
    }
}

pub async fn quick_backup(app: &AppHandle, t: QuickActionType) {
    info!(target:"rgsm::quick_action", "Auto backup triggered: {:#?}", t.generate_describe());
    let config = match get_config() {
        Ok(config) => config,
        Err(err) => {
            error!(target:"rgsm::quick_action", "Failed to load config: {err:?}");
            return;
        }
    };

    let prompt_when_auto_backup = config.settings.prompt_when_auto_backup;
    let quick_settings = config.quick_action.clone();
    let sound_preferences: QuickActionSoundPreferences =
        QuickActionSoundPreferences::from(&quick_settings);

    // 检查游戏是否已选择
    let game = match quick_settings.quick_action_game.clone() {
        Some(game) => game,
        None => {
            emit_quick_action_event(
                app,
                t,
                QuickActionOperation::Backup,
                QuickActionStatus::Failure,
                None,
            );
            show_no_game_selected_error(app, &quick_settings, &sound_preferences);
            return;
        }
    };

    // 执行备份操作
    let result = game.create_snapshot(&t.generate_describe()).await;

    // 处理结果
    match result {
        Err(e) => {
            error!(target:"rgsm::quick_action", "Quick backup failed: {:#?}", &e);
            maybe_show_notification(
                &quick_settings,
                t!("backend.tray.error"),
                format!("{:#?}\n{:#?}", t!("backend.tray.find_error_detail"), e),
            );
            play_quick_action_sound(app, sound_preferences, QuickActionSoundEffect::Failure);
            emit_quick_action_event(
                app,
                t,
                QuickActionOperation::Backup,
                QuickActionStatus::Failure,
                Some(game.name.clone()),
            );
        }
        Ok(_) => {
            maybe_show_success_notification(
                &quick_settings,
                prompt_when_auto_backup || t != QuickActionType::Timer,
                t!("backend.tray.success"),
                format!(
                    "{:#?} {} {}",
                    game.name,
                    t!("backend.tray.quick_backup"),
                    t!("backend.tray.success")
                ),
            );
            play_quick_action_sound(app, sound_preferences, QuickActionSoundEffect::Success);
            emit_quick_action_event(
                app,
                t,
                QuickActionOperation::Backup,
                QuickActionStatus::Success,
                Some(game.name.clone()),
            );
        }
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

fn maybe_show_success_notification<T1: AsRef<str>, T2: AsRef<str>>(
    settings: &QuickActionsSettings,
    should_notify: bool,
    title: T1,
    body: T2,
) {
    if settings.enable_notification && should_notify {
        show_notification(title, body);
    }
}
