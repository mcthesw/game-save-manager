use std::{path::PathBuf, sync::Arc};

use log::info;
use tauri::{
    AppHandle, LogicalSize, Manager, State,
    menu::{MenuBuilder, MenuEvent, MenuItemBuilder, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    utils::config::WindowConfig,
};
use tauri_plugin_window_state::{StateFlags, WindowExt};

use crate::config::get_config;

use super::{AutoBackupDuration, QuickActionType, quick_apply, quick_backup};

use rust_i18n::t;

// TODO:处理错误
pub fn setup_tray(app: &mut tauri::App) -> anyhow::Result<()> {
    info!(target: "rgsm::quick_action::tray", "Setting up tray icon");
    let config = get_config()?;

    // Menu items begin
    let current_quick_action_game =
        MenuItemBuilder::new(config.quick_action.quick_action_game.map_or_else(
            || t!("backend.tray.no_game_selected"),
            |game| game.name.into(),
        ))
        .id("game")
        .enabled(true)
        .build(app)?;
    let timer_backup = SubmenuBuilder::new(app, t!("backend.tray.auto_backup_interval"))
        .items(&[
            &MenuItemBuilder::new(t!("backend.tray.turn_off_auto_backup"))
                .id("timer.0")
                .build(app)?,
            &MenuItemBuilder::new(t!("backend.tray.5_minute"))
                .id("timer.5")
                .build(app)?,
            &MenuItemBuilder::new(t!("backend.tray.10_minute"))
                .id("timer.10")
                .build(app)?,
            &MenuItemBuilder::new(t!("backend.tray.30_minute"))
                .id("timer.30")
                .build(app)?,
            &MenuItemBuilder::new(t!("backend.tray.60_minute"))
                .id("timer.60")
                .build(app)?,
        ])
        .build()?;
    // Menu items end

    let tray_menu = MenuBuilder::new(app)
        .items(&[
            &current_quick_action_game,
            &timer_backup,
            &MenuItemBuilder::new(t!("backend.tray.quick_backup"))
                .id("backup")
                .build(app)?,
            &MenuItemBuilder::new(t!("backend.tray.quick_apply"))
                .id("apply")
                .build(app)?,
            &MenuItemBuilder::new(t!("backend.tray.exit"))
                .id("quit")
                .build(app)?,
        ])
        .build()?;

    TrayIconBuilder::with_id("tray_icon")
        .icon(app.default_window_icon().unwrap().clone())
        .show_menu_on_left_click(false)
        .menu(&tray_menu)
        .on_tray_icon_event(tray_event_handler)
        .on_menu_event(menu_event_handler)
        .build(app)?;

    info!(target: "rgsm::quick_action::tray", "Tray icon created");
    Ok(())
}

pub fn tray_event_handler(tray: &TrayIcon, event: TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        // 单击托盘图标时，显示主窗口（若主窗口不存在）
        info!(target: "rgsm::quick_action::tray", "Tray left click");
        let app = tray.app_handle();
        if app.get_webview_window("main").is_none() {
            let window = tauri::WebviewWindowBuilder::from_config(
                app,
                &WindowConfig {
                    label: "main".to_string(),
                    url: tauri::WebviewUrl::App(PathBuf::from("index.html")),
                    drag_drop_enabled: false, // 必须这样设置，否则窗体内js接收不到drag & drop事件
                    title: "RustyManager".to_string(),
                    ..Default::default()
                },
            )
            .unwrap()
            .build()
            .unwrap();

            window
                .restore_state(StateFlags::all())
                .expect("Cannot restore window state");
            window.show().expect("Cannot show window");
            window.set_focus().expect("Cannot set focus");
        }
    }
}

pub fn menu_event_handler(app: &AppHandle, event: MenuEvent) {
    match event.id.as_ref() {
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
                let duration = other
                    .split('.')
                    .next_back()
                    .unwrap()
                    .parse::<u32>()
                    .unwrap();
                let state: State<Arc<AutoBackupDuration>> = app.state();
                state.store(duration, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
}
