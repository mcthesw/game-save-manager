use crate::{
    config::{QuickActionSoundSettings, get_config},
    preclude::*,
};
use log::{error, info, warn};
use rust_i18n::t;

#[derive(Debug, PartialEq)]
pub enum QuickActionType {
    Timer,
    Tray,
    Hotkey,
}

impl QuickActionType {
    fn generate_describe(&self) -> String {
        match &self {
            QuickActionType::Timer => String::from("Auto Backup (Timer)"),
            QuickActionType::Tray => String::from("Quick Backup (Tray)"),
            QuickActionType::Hotkey => String::from("Quick Backup (Hotkey)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickActionResult {
    Success,
    Failed,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct QuickActionOutcome {
    pub result: QuickActionResult,
    pub sound_settings: QuickActionSoundSettings,
}

pub async fn quick_apply(t: QuickActionType) -> QuickActionOutcome {
    info!(target:"rgsm::quick_action", "Auto apply triggered: {:#?}", t.generate_describe());
    let config = match get_config() {
        Ok(config) => config,
        Err(err) => {
            error!(target:"rgsm::quick_action", "Failed to load config for quick apply: {err:?}");
            return QuickActionOutcome {
                result: QuickActionResult::Failed,
                sound_settings: QuickActionSoundSettings::default(),
            };
        }
    };

    let sound_settings = config.quick_action.sound.clone();
    let notify_success = sound_settings.notifications.on_success;
    let notify_error = sound_settings.notifications.on_error;

    let game = match config.quick_action.quick_action_game.clone() {
        Some(game) => game,
        None => {
            if notify_error {
                show_no_game_selected_error();
            }
            return QuickActionOutcome {
                result: QuickActionResult::Skipped,
                sound_settings,
            };
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
            if notify_error {
                show_notification(
                    t!("backend.tray.error"),
                    format!("{:#?}\n{:#?}", t!("backend.tray.find_error_detail"), e),
                );
            }
            QuickActionOutcome {
                result: QuickActionResult::Failed,
                sound_settings,
            }
        }
        Ok(_) => {
            if notify_success {
                show_notification(
                    t!("backend.tray.success"),
                    format!(
                        "{:#?} {} {}",
                        game.name,
                        t!("backend.tray.quick_apply"),
                        t!("backend.tray.success")
                    ),
                );
            }
            QuickActionOutcome {
                result: QuickActionResult::Success,
                sound_settings,
            }
        }
    }
}

pub async fn quick_backup(t: QuickActionType) -> QuickActionOutcome {
    info!(target:"rgsm::quick_action", "Auto backup triggered: {:#?}", t.generate_describe());
    let config = match get_config() {
        Ok(config) => config,
        Err(err) => {
            error!(target:"rgsm::quick_action", "Failed to load config for quick backup: {err:?}");
            return QuickActionOutcome {
                result: QuickActionResult::Failed,
                sound_settings: QuickActionSoundSettings::default(),
            };
        }
    };

    let sound_settings = config.quick_action.sound.clone();
    let notify_success = should_notify_backup_success(&sound_settings, &config, t);
    let notify_error = sound_settings.notifications.on_error;

    let game = match config.quick_action.quick_action_game.clone() {
        Some(game) => game,
        None => {
            if notify_error {
                show_no_game_selected_error();
            }
            return QuickActionOutcome {
                result: QuickActionResult::Skipped,
                sound_settings,
            };
        }
    };

    // 执行备份操作
    let result = game.create_snapshot(&t.generate_describe()).await;

    // 处理结果
    match result {
        Err(e) => {
            error!(target:"rgsm::quick_action", "Quick backup failed: {:#?}", &e);
            if notify_error {
                show_notification(
                    t!("backend.tray.error"),
                    format!("{:#?}\n{:#?}", t!("backend.tray.find_error_detail"), e),
                );
            }
            QuickActionOutcome {
                result: QuickActionResult::Failed,
                sound_settings,
            }
        }
        Ok(_) => {
            if notify_success {
                show_notification(
                    t!("backend.tray.success"),
                    format!(
                        "{:#?} {} {}",
                        game.name,
                        t!("backend.tray.quick_backup"),
                        t!("backend.tray.success")
                    ),
                );
            }
            QuickActionOutcome {
                result: QuickActionResult::Success,
                sound_settings,
            }
        }
    }
}

fn should_notify_backup_success(
    sound_settings: &QuickActionSoundSettings,
    config: &crate::config::Config,
    trigger: QuickActionType,
) -> bool {
    if !sound_settings.notifications.on_success {
        return false;
    }

    if trigger == QuickActionType::Timer {
        config.settings.prompt_when_auto_backup
    } else {
        true
    }
}

fn show_no_game_selected_error() {
    warn!(target:"rgsm::quick_action", "No game selected, cannot quick backup/apply");
    show_notification(
        t!("backend.tray.error"),
        t!("backend.tray.no_game_selected"),
    );
}
