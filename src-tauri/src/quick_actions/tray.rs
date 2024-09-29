use std::sync::Arc;

use tauri::{
    utils::config::WindowConfig, AppHandle, CustomMenuItem, LogicalSize, Manager, State,
    SystemTray, SystemTrayEvent, SystemTrayMenu, SystemTrayMenuItem, SystemTraySubmenu,
};
use tracing::info;

use crate::config::get_config;

use super::{quick_apply, quick_backup, AutoBackupDuration, QuickActionType};

use rust_i18n::t;

// TODO:处理错误
pub fn get_tray() -> SystemTray {
    let config = get_config().expect("Cannot get config");
    let current_quick_action_game = match config.quick_action.quick_action_game {
        Some(game) => CustomMenuItem::new("game".to_owned(), game.name),
        None => CustomMenuItem::new("game".to_owned(), t!("backend.tray.no_game_selected")),
    };
    // Menu items begin
    let tray_menu = SystemTrayMenu::new()
        .add_item(current_quick_action_game)
        .add_submenu(SystemTraySubmenu::new(
            t!("backend.tray.auto_backup_interval"),
            SystemTrayMenu::new()
                .add_item(CustomMenuItem::new(
                    "timer.0".to_owned(),
                    t!("backend.tray.turn_off_auto_backup"),
                ))
                .add_item(CustomMenuItem::new(
                    "timer.5".to_owned(),
                    t!("backend.tray.5_minute"),
                ))
                .add_item(CustomMenuItem::new(
                    "timer.10".to_owned(),
                    t!("backend.tray.10_minute"),
                ))
                .add_item(CustomMenuItem::new(
                    "timer.30".to_owned(),
                    t!("backend.tray.30_minute"),
                ))
                .add_item(CustomMenuItem::new(
                    "timer.60".to_owned(),
                    t!("backend.tray.60_minute"),
                )),
        ))
        .add_item(CustomMenuItem::new(
            "backup".to_owned(),
            t!("backend.tray.quick_backup"),
        ))
        .add_item(CustomMenuItem::new(
            "apply".to_owned(),
            t!("backend.tray.quick_apply"),
        ))
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(CustomMenuItem::new(
            "quit".to_owned(),
            t!("backend.tray.exit"),
        ));
    // Menu items end

    SystemTray::new().with_menu(tray_menu)
}

pub fn tray_event_handler(app: &AppHandle, event: SystemTrayEvent) {
    match event {
        SystemTrayEvent::LeftClick { .. } => {
            info!(target: "rgsm::quick_action::tray", "Tray left click");
            if let Some(window) = app.get_window("main") {
                window.close().expect("Cannot close window");
            } else {
                let window = tauri::WindowBuilder::from_config(
                    app,
                    WindowConfig {
                        label: "main".to_string(),
                        url: tauri::WindowUrl::App("index.html".into()),
                        file_drop_enabled: false, // 必须这样设置，否则窗体内js接收不到drag & drop事件
                        title: "RustyManager".to_string(),
                        ..Default::default()
                    },
                )
                .build()
                .unwrap();

                window
                    .set_size(LogicalSize {
                        width: 1280.0,
                        height: 720.0,
                    })
                    .expect("Cannot set size");
                window.show().expect("Cannot show window");
                window.set_focus().expect("Cannot set focus");
            }
        }
        SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
            "backup" => {
                info!(target:"rgsm::quick_action::tray", "Tray quick backup clicked");
                tauri::async_runtime::spawn(async move {
                    quick_backup(QuickActionType::Tray).await;
                });
            }
            "apply" => {
                info!(target:"rgsm::quick_action::tray", "Tray quick apply clicked.");
                tauri::async_runtime::spawn(async move {
                    quick_apply(QuickActionType::Tray).await;
                });
            }
            "quit" => {
                info!(target:"rgsm::quick_action::tray","Tray quit clicked.");
                app.exit(0);
            }
            other => {
                // other情况一定是选择定时备份的时间
                info!(target:"rgsm::quick_action::tray","Tray menu item clicked: {other}.");
                if other.starts_with("timer.") {
                    // safe:所有输入来自程序字面量，保证了不会出现非数字的情况
                    let duration = other.split('.').last().unwrap().parse::<u32>().unwrap();
                    let state: State<Arc<AutoBackupDuration>> = app.state();
                    state.store(duration, std::sync::atomic::Ordering::Relaxed);
                }
            }
        },
        _ => {}
    }
}
