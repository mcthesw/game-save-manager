use crate::{backup::Game, config::get_config, preclude::*};
use log::{error, info, warn};
use rust_i18n::t;

use super::sound::{QuickActionSoundEvent, play_sound};

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

pub async fn quick_apply(t: QuickActionType) {
    info!(target:"rgsm::quick_action", "Auto apply triggered: {:#?}", t.generate_describe());

    let config = get_config().expect("Cannot get config");
    let notifications_enabled = config.quick_action.notifications.enabled;
    let game = match config.quick_action.quick_action_game.clone() {
        Some(game) => game,
        None => {
            show_no_game_selected_error(notifications_enabled);
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
            play_sound(QuickActionSoundEvent::Failure);
            if notifications_enabled {
                show_notification(
                    t!("backend.tray.error"),
                    format!("{:#?}\n{:#?}", t!("backend.tray.find_error_detail"), e),
                );
            }
        }
        Ok(_) => {
            play_sound(QuickActionSoundEvent::Success);
            if notifications_enabled {
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
        }
    }
}

pub async fn quick_backup(t: QuickActionType) {
    info!(target:"rgsm::quick_action", "Auto backup triggered: {:#?}", t.generate_describe());
    let config = get_config().expect("Cannot get config");
    let prompt_timer_notification = config.settings.prompt_when_auto_backup;
    let notifications_enabled = config.quick_action.notifications.enabled;

    // 检查游戏是否已选择
    let game = match config.quick_action.quick_action_game.clone() {
        Some(game) => game,
        None => {
            show_no_game_selected_error(notifications_enabled);
            return;
        }
    };

    // 执行备份操作
    let result = game.create_snapshot(&t.generate_describe()).await;

    // 处理结果
    match result {
        Err(e) => {
            error!(target:"rgsm::quick_action", "Quick backup failed: {:#?}", &e);
            play_sound(QuickActionSoundEvent::Failure);
            if notifications_enabled {
                show_notification(
                    t!("backend.tray.error"),
                    format!("{:#?}\n{:#?}", t!("backend.tray.find_error_detail"), e),
                );
            }
        }
        Ok(_) => {
            play_sound(QuickActionSoundEvent::Success);
            let should_notify =
                notifications_enabled && (t != QuickActionType::Timer || prompt_timer_notification);
            if !should_notify {
                return;
            }
            // 根据设置决定是否显示通知
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
    }
}

fn show_no_game_selected_error(notifications_enabled: bool) {
    warn!(target:"rgsm::quick_action", "No game selected, cannot quick backup/apply");
    play_sound(QuickActionSoundEvent::Failure);
    if notifications_enabled {
        show_notification(
            t!("backend.tray.error"),
            t!("backend.tray.no_game_selected"),
        );
    }
}
