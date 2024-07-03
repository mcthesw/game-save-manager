use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tauri::{
    api::notification::Notification, utils::config::WindowConfig, App, AppHandle, CustomMenuItem,
    LogicalSize, Manager, State, SystemTray, SystemTrayEvent, SystemTrayMenu, SystemTrayMenuItem,
    SystemTraySubmenu,
};
use tracing::{info, warn};

use crate::config::{get_config, Game};

use rust_i18n::t;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct QuickBackupState {
    pub current_game: Option<Game>,
    /// 自动备份的间隔，以分钟为单位
    pub auto_backup_duration: Option<u32>,
}
impl QuickBackupState {
    pub fn default() -> QuickBackupState {
        QuickBackupState {
            current_game: None,
            auto_backup_duration: None,
        }
    }
}

pub fn get_tray() -> SystemTray {
    // Menu items begin
    let tray_menu = SystemTrayMenu::new()
        .add_item(CustomMenuItem::new(
            "game".to_owned(),
            t!("tray.no_game_selected"),
        ))
        .add_submenu(SystemTraySubmenu::new(
            t!("tray.auto_backup_interval"),
            SystemTrayMenu::new()
                .add_item(CustomMenuItem::new(
                    "timer.0".to_owned(),
                    t!("tray.turn_off_auto_backup"),
                ))
                .add_item(CustomMenuItem::new(
                    "timer.5".to_owned(),
                    t!("tray.5_minute"),
                ))
                .add_item(CustomMenuItem::new(
                    "timer.10".to_owned(),
                    t!("tray.10_minute"),
                ))
                .add_item(CustomMenuItem::new(
                    "timer.30".to_owned(),
                    t!("tray.30_minute"),
                ))
                .add_item(CustomMenuItem::new(
                    "timer.60".to_owned(),
                    t!("tray.60_minute"),
                )),
        ))
        .add_item(CustomMenuItem::new(
            "backup".to_owned(),
            t!("tray.quick_backup"),
        ))
        .add_item(CustomMenuItem::new(
            "apply".to_owned(),
            t!("tray.quick_apply"),
        ))
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(CustomMenuItem::new("quit".to_owned(), t!("tray.exit")));
    // Menu items end

    SystemTray::new().with_menu(tray_menu)
}

pub fn tray_event_handler(app: &AppHandle, event: SystemTrayEvent) {
    match event {
        SystemTrayEvent::LeftClick { .. } => {
            info!(target: "rgsm::tray", "Tray left click");
            if let Some(window) = app.get_window("main") {
                window.close().expect("Cannot close window");
            } else {
                let window = tauri::WindowBuilder::from_config(
                    app,
                    WindowConfig {
                        label: "main".to_string(),
                        url: tauri::WindowUrl::App("index.html".into()),
                        file_drop_enabled: false, // 必须这样设置，否则窗体内js接收不到drag & drop事件
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
                info!(target:"rgsm::tray", "Tray quick backup clicked");
                let state: State<Arc<Mutex<QuickBackupState>>> = app.state();
                let game = &state.lock().expect("Cannot get state lock").current_game;
                match game {
                    Some(game) => {
                        info!(target:"rgsm::tray", "Quick backup game: {:#?}", game);
                        tauri::async_runtime::block_on(async {
                            game.backup_save("Quick Backup").await
                        })
                        .expect("Tauri async runtime error, cannot block_on");
                        Notification::new(&app.config().tauri.bundle.identifier)
                            .title(t!("tray.success"))
                            .body(format!(
                                "{:#?} {} {}",
                                game.name,
                                t!("tray.quick_backup"),
                                t!("tray.success")
                            ))
                            .show()
                            .expect("Cannot show notification");
                    }
                    None => {
                        warn!(target:"rgsm::tray", "No game selected, cannot quick backup");
                        Notification::new(&app.config().tauri.bundle.identifier)
                            .title(t!("tray.error"))
                            .body(t!("tray.no_game_selected"))
                            .show()
                            .expect("Cannot show notification");
                    }
                }
            }
            "apply" => {
                info!(target:"rgsm::tray", "Tray quick apply clicked.");
                let state: State<Arc<Mutex<QuickBackupState>>> = app.state();
                let game = &state.lock().expect("Cannot get state lock").current_game;
                match game {
                    Some(game) => {
                        info!(target:"rgsm::tray", "Quick apply game: {:#?}", game);
                        let newest_date = game
                            .get_backup_list_info()
                            .expect("Cannot get backup list info")
                            .backups
                            .last()
                            .expect("No backup available")
                            .date
                            .clone();
                        tauri::async_runtime::block_on(async {
                            game.apply_backup(&newest_date, app)
                        })
                        .expect("Tauri async runtime error, cannot block_on");
                        Notification::new(&app.config().tauri.bundle.identifier)
                            .title(t!("tray.success"))
                            .body(format!(
                                "{:#?} {} {}",
                                game.name,
                                t!("tray.quick_apply"),
                                t!("tray.success")
                            ))
                            .show()
                            .expect("Cannot show notification");
                    }
                    None => {
                        warn!(target:"rgsm::tray","No game selected, cannot quick apply.");
                        Notification::new(&app.config().tauri.bundle.identifier)
                            .title(t!("tray.error"))
                            .body(t!("tray.no_game_selected"))
                            .show()
                            .expect("Cannot show notification");
                    }
                }
            }
            "quit" => {
                info!(target:"rgsm::tray","Tray quit clicked.");
                app.exit(0);
            }
            other => {
                info!(target:"rgsm::tray","Tray menu item clicked: {other}.");
                if other.starts_with("timer.") {
                    // safe:所有输入来自程序字面量，保证了不会出现非数字的情况
                    let duration = match other.split('.').last().unwrap() {
                        "0" => None,
                        // safe:所有输入来自程序字面量，保证了不会出现非数字的情况
                        other => Some(other.parse::<u32>().unwrap()),
                    };
                    let state: State<Arc<Mutex<QuickBackupState>>> = app.state();
                    state
                        .lock()
                        .expect("Cannot get state lock")
                        .auto_backup_duration = duration;
                }
            }
        },
        _ => {}
    }
}

pub fn set_current_game(app: &AppHandle, game: Game) {
    info!(target:"rgsm::tray","Setting current quick backup game:{}",game.name);
    let state: State<Arc<Mutex<QuickBackupState>>> = app.state();
    app.tray_handle()
        .get_item("game")
        .set_title(&game.name)
        .expect("Cannot get tray handle");
    state.lock().expect("Cannot get state lock").current_game = Some(game);
}

pub fn setup_timer(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    info!(target:"rgsm::tray::timer","Setting up tray timer.");
    let state: State<Arc<Mutex<QuickBackupState>>> = app.state();
    let state = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        let mut counter = 0;
        let mut last = None;
        loop {
            let (duration, game) = {
                let state_guard = state.lock().expect("Cannot get state lock");
                (
                    state_guard.auto_backup_duration,
                    state_guard.current_game.clone(),
                )
            };
            if last != duration {
                // 如果发生改变，重新计数
                counter = 1;
            }

            if let Some(duration) = duration {
                if counter >= duration {
                    info!(target:"rgsm::tray::timer", "Auto backup triggered.");
                    match &game {
                        Some(game) => {
                            info!(target:"rgsm::tray::timer", "Backing up game:{}",game.name);
                            let show_info = get_config()
                                .expect("Cannot get config")
                                .settings
                                .prompt_when_auto_backup;
                            game.backup_save("Auto Backup Info")
                                .await
                                .expect("Cannot backup");
                            if show_info {
                                Notification::new("QuickBackup")
                                    .title(t!("tray.success"))
                                    .body(format!("{:#?}自动备份成功", game.name))
                                    .show()
                                    .expect("Cannot show notification");
                            }
                        }
                        None => {
                            warn!(target:"rgsm::tray::timer", "No game selected, skipping auto backup.");
                            Notification::new("Auto Backup Info")
                                .title(t!("tray.error"))
                                .body(t!("tray.no_game_selected"))
                                .show()
                                .expect("Cannot show notification");
                        }
                    }
                    counter = 0;
                }
            }
            last = duration;
            std::thread::sleep(Duration::from_secs(60));
            counter += 1;
            counter %= u32::MAX; // 防止溢出
        }
    });
    info!(target:"rgsm::tray::timer","Tray timer setup complete.");
    Ok(())
}
