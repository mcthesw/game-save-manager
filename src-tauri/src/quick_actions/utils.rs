use tauri::{api::notification::Notification, AppHandle};
use tracing::{info, warn};

use crate::{
    backup::Game,
    config::{get_config, set_config},
};

use super::*;

pub async fn set_current_game(app: &AppHandle, game: Game) {
    info!(target:"rgsm::tray","Setting current quick backup game:{}",game.name);
    app.tray_handle()
        .get_item("game")
        .set_title(&game.name)
        .expect("Cannot get tray handle");
    let mut config = get_config().expect("Cannot get config");
    config.quick_action.quick_action_game = Some(game);
    set_config(&config).await.expect("Cannot set config");
}

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
    info!(target:"rgsm::quick_action", "Auto apply triggered: {:#?}",t.generate_describe());
    let game = get_quick_action_game();
    match game {
        Some(game) => {
            info!(target:"rgsm::quick_action", "Quick apply game: {:#?}", game);
            let newest_date = game
                .get_game_snapshots_info()
                .expect("Cannot get backup list info")
                .backups
                .last()
                .expect("No backup available")
                .date
                .clone();
            game.restore_snapshot(&newest_date, None)
                .expect("Cannot apply");
            Notification::new("QuickAction")
                .title(t!("backend.tray.success"))
                .body(format!(
                    "{:#?} {} {}",
                    game.name,
                    t!("backend.tray.quick_apply"),
                    t!("backend.tray.success")
                ))
                .show()
                .expect("Cannot show notification");
        }
        None => show_no_game_selected_error(),
    }
}

pub async fn quick_backup(t: QuickActionType) {
    info!(target:"rgsm::quick_action", "Auto backup triggered: {:#?}",t.generate_describe());
    let game = get_quick_action_game();
    // TODO:这里可以让match有返回值来判断是否出错
    match &game {
        None => show_no_game_selected_error(),
        Some(game) => {
            let show_info = get_config()
                .expect("Cannot get config")
                .settings
                .prompt_when_auto_backup;
            game.create_snapshot(&t.generate_describe())
                .await
                .expect("Cannot backup");
            if !show_info && (t == QuickActionType::Timer) {
                // 设置中该选项控制是否在按间隔备份时发出通知
                // 若不启用，则不进行通知，其余情况则产生通知
                return;
            }
            Notification::new("QuickAction")
                .title(t!("backend.tray.success"))
                .body(format!(
                    "{:#?} {} {}",
                    game.name,
                    t!("backend.tray.quick_backup"),
                    t!("backend.tray.success")
                ))
                .show()
                .expect("Cannot show notification");
        }
    }
}

fn show_no_game_selected_error() {
    warn!(target:"rgsm::quick_action", "No game selected, cannot quick backup/apply");
    Notification::new("QuickAction")
        .title(t!("backend.tray.error"))
        .body(t!("backend.tray.no_game_selected"))
        .show()
        .expect("Cannot show notification");
}

pub fn get_quick_action_game() -> Option<Game> {
    get_config()
        .expect("Cannot get config")
        .quick_action
        .quick_action_game
        .clone()
}

pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let config = get_config()?;
    timer::setup_timer(app)?;
    hotkeys::setup_hotkeys(&config, app)?;
    Ok(())
}
