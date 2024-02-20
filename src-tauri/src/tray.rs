// TODO: Need i18n:https://crates.io/crates/rust-i18n
// TODO: 需要错误处理和日志，这里的东西大多在其他线程，无法打印到主线程的控制台
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tauri::{
    api::notification::Notification, App, AppHandle, CustomMenuItem, LogicalSize, Manager, State,
    SystemTray, SystemTrayEvent, SystemTrayMenu, SystemTrayMenuItem, SystemTraySubmenu,
};

use crate::config::{get_config, Game};

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
        .add_item(CustomMenuItem::new("game".to_owned(), "未选择游戏"))
        .add_submenu(SystemTraySubmenu::new(
            "自动备份间隔",
            SystemTrayMenu::new()
                .add_item(CustomMenuItem::new("timer.0".to_owned(), "关闭自动备份"))
                .add_item(CustomMenuItem::new("timer.5".to_owned(), "5分钟"))
                .add_item(CustomMenuItem::new("timer.10".to_owned(), "10分钟"))
                .add_item(CustomMenuItem::new("timer.30".to_owned(), "30分钟"))
                .add_item(CustomMenuItem::new("timer.60".to_owned(), "60分钟")),
        ))
        .add_item(CustomMenuItem::new("backup".to_owned(), "快速备份"))
        .add_item(CustomMenuItem::new("apply".to_owned(), "快速读档"))
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(CustomMenuItem::new("quit".to_owned(), "退出"));
    // Menu items end

    SystemTray::new().with_menu(tray_menu)
}

pub fn tray_event_handler(app: &AppHandle, event: SystemTrayEvent) {
    match event {
        SystemTrayEvent::LeftClick { .. } => {
            if let Some(window) = app.get_window("main") {
                window.close().expect("Cannot close window");
            } else {
                let window = tauri::WindowBuilder::new(
                    app,
                    "main",
                    tauri::WindowUrl::App("index.html".into()),
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
            // if window.is_visible().expect("Cannot get is_visible") {
            //     window.hide().expect("Cannot hide");
            // } else {
            //     window.show().expect("Cannot show");
            //     window.set_focus().expect("Cannot set focus");
            // }
        }
        SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
            "backup" => {
                let state: State<Arc<Mutex<QuickBackupState>>> = app.state();
                let game = &state.lock().expect("Cannot get state lock").current_game;
                match game {
                    Some(game) => {
                        tauri::async_runtime::block_on(async {
                            game.backup_save("Quick Backup").await
                        })
                        .expect("Tauri async runtime error, cannot block_on");
                        Notification::new(&app.config().tauri.bundle.identifier)
                            .title("成功")
                            .body(format!("{:#?}备份成功", game.name))
                            .show()
                            .expect("Cannot show notification");
                    }
                    None => {
                        Notification::new(&app.config().tauri.bundle.identifier)
                            .title("错误")
                            .body("未选择游戏")
                            .show()
                            .expect("Cannot show notification");
                    }
                }
            }
            "apply" => {
                let state: State<Arc<Mutex<QuickBackupState>>> = app.state();
                let game = &state.lock().expect("Cannot get state lock").current_game;
                match game {
                    Some(game) => {
                        let newest_date = game
                            .get_backup_list_info()
                            .expect("Cannot get backup list info")
                            .backups[0]
                            .date
                            .clone();
                        tauri::async_runtime::block_on(async { game.apply_backup(&newest_date) })
                            .expect("Tauri async runtime error, cannot block_on");
                        Notification::new(&app.config().tauri.bundle.identifier)
                            .title("成功")
                            .body(format!("{:#?}应用成功", game.name))
                            .show()
                            .expect("Cannot show notification");
                    }
                    None => {
                        Notification::new(&app.config().tauri.bundle.identifier)
                            .title("错误")
                            .body("未选择游戏")
                            .show()
                            .expect("Cannot show notification");
                    }
                }
            }
            "quit" => {
                app.exit(0);
            }
            other => {
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
    let state: State<Arc<Mutex<QuickBackupState>>> = app.state();
    app.tray_handle()
        .get_item("game")
        .set_title(&game.name)
        .expect("Cannot get tray handle");
    state.lock().expect("Cannot get state lock").current_game = Some(game);
}

pub fn setup_timer(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let state: State<Arc<Mutex<QuickBackupState>>> = app.state();
    let state = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        let mut counter = 0;
        let mut last = None;
        // 需要初始化Notification，否则第一次提示不会显示
        Notification::new("Auto Backup Info")
            .title("初始化")
            .body("初始化Notification")
            .show()
            .expect("Cannot show notification");
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
                    match &game {
                        Some(game) => {
                            let show_info = get_config()
                                .expect("Cannot get config")
                                .settings
                                .prompt_when_auto_backup;
                            game.backup_save("Auto Backup Info")
                                .await
                                .expect("Cannot backup");
                            if show_info {
                                Notification::new("QuickBackup")
                                    .title("成功")
                                    .body(format!("{:#?}自动备份成功", game.name))
                                    .show()
                                    .expect("Cannot show notification");
                            }
                        }
                        None => {
                            Notification::new("Auto Backup Info")
                                .title("错误")
                                .body("未选择游戏")
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
    Ok(())
}
