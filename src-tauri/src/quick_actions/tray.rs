use std::sync::atomic::Ordering;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use log::{info, warn};
use tauri::{
    AppHandle, Manager, State, Wry,
    menu::{
        CheckMenuItem, CheckMenuItemBuilder, MenuBuilder, MenuEvent, MenuItemBuilder,
        SubmenuBuilder,
    },
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    utils::config::WindowConfig,
};
use tauri_plugin_window_state::{StateFlags, WindowExt};

use crate::config::get_config;

use super::{AutoBackupDuration, QuickActionType, quick_apply, quick_backup};

use rust_i18n::t;

#[derive(Default)]
pub struct AutoBackupMenuState {
    items: Mutex<HashMap<u32, CheckMenuItem<Wry>>>,
}

impl AutoBackupMenuState {
    pub fn replace_items(&self, items: HashMap<u32, CheckMenuItem<Wry>>) {
        match self.items.lock() {
            Ok(mut guard) => *guard = items,
            Err(err) => warn!(
                target: "rgsm::quick_action::tray",
                "Failed to update auto backup menu items: {err}"
            ),
        }
    }

    pub fn mark_selected(&self, selected: u32) {
        match self.items.lock() {
            Ok(guard) => {
                for (duration, item) in guard.iter() {
                    if let Err(err) = item.set_checked(*duration == selected) {
                        warn!(
                            target: "rgsm::quick_action::tray",
                            "Failed to set check state for timer.{duration}: {err:?}"
                        );
                    }
                }
            }
            Err(err) => warn!(
                target: "rgsm::quick_action::tray",
                "Failed to access auto backup menu items: {err}"
            ),
        }
    }
}

// TODO:处理错误
pub fn setup_tray(app: &mut tauri::App) -> anyhow::Result<()> {
    info!(target: "rgsm::quick_action::tray", "Setting up tray icon");
    let config = get_config()?;
    let duration_state: State<Arc<AutoBackupDuration>> = app.state();
    let selected_duration = duration_state.load(Ordering::Acquire);

    // Menu items begin
    let current_quick_action_game =
        MenuItemBuilder::new(config.quick_action.quick_action_game.map_or_else(
            || t!("backend.tray.no_game_selected"),
            |game| game.name.into(),
        ))
        .id("game")
        .enabled(true)
        .build(app)?;
    let timer_options = [
        (0_u32, t!("backend.tray.turn_off_auto_backup")),
        (5_u32, t!("backend.tray.5_minute")),
        (10_u32, t!("backend.tray.10_minute")),
        (30_u32, t!("backend.tray.30_minute")),
        (60_u32, t!("backend.tray.60_minute")),
    ];

    let mut timer_items = Vec::with_capacity(timer_options.len());
    let mut timer_item_map = HashMap::with_capacity(timer_options.len());
    for (duration, label) in timer_options.into_iter() {
        let item = CheckMenuItemBuilder::new(label)
            .id(format!("timer.{duration}"))
            .checked(selected_duration == duration)
            .build(app)?;
        timer_item_map.insert(duration, item.clone());
        timer_items.push(item);
    }

    let timer_item_refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = timer_items
        .iter()
        .map(|item| item as &dyn tauri::menu::IsMenuItem<Wry>)
        .collect();

    let timer_backup = SubmenuBuilder::new(app, t!("backend.tray.auto_backup_interval"))
        .items(timer_item_refs.as_slice())
        .build()?;

    let menu_state: State<Arc<AutoBackupMenuState>> = app.state();
    menu_state.replace_items(timer_item_map);
    menu_state.mark_selected(selected_duration);
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
                let parsed_duration = other
                    .split('.')
                    .next_back()
                    .and_then(|value| value.parse::<u32>().ok());

                if let Some(duration) = parsed_duration {
                    let state: State<Arc<AutoBackupDuration>> = app.state();
                    state.store(duration, Ordering::Release);

                    if let Some(menu_state) = app.try_state::<Arc<AutoBackupMenuState>>() {
                        menu_state.mark_selected(duration);
                    }
                } else {
                    warn!(
                        target:"rgsm::quick_action::tray",
                        "Failed to parse timer duration from menu id: {other}"
                    );
                }
            }
        }
    }
}
