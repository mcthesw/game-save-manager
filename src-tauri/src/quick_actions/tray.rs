use std::{collections::HashMap, path::PathBuf, sync::Arc};

use log::info;
use tauri::{
    AppHandle, Manager, State, Wry,
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuEvent, MenuItemBuilder, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    utils::config::WindowConfig,
};
use tauri_plugin_window_state::{StateFlags, WindowExt};

use super::{QuickActionManager, QuickActionType};

use rust_i18n::t;

pub fn setup_tray(app: &mut tauri::App) -> anyhow::Result<()> {
    info!(target: "rgsm::quick_action::tray", "Setting up tray icon");

    let manager_state: State<Arc<QuickActionManager>> = app.state();
    let manager = Arc::clone(manager_state.inner());

    let selected_duration = manager.current_interval();
    let current_game_label = manager
        .current_game()
        .map(|game| game.name)
        .unwrap_or_else(|| t!("backend.tray.no_game_selected").into());

    let current_quick_action_game = MenuItemBuilder::new(current_game_label)
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

    manager.register_tray_items(current_quick_action_game.clone(), timer_item_map);

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
        info!(target: "rgsm::quick_action::tray", "Tray left click");
        let app = tray.app_handle();
        if app.get_webview_window("main").is_none() {
            let window = tauri::WebviewWindowBuilder::from_config(
                app,
                &WindowConfig {
                    label: "main".to_string(),
                    url: tauri::WebviewUrl::App(PathBuf::from("index.html")),
                    drag_drop_enabled: false,
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
    let manager_state: State<Arc<QuickActionManager>> = app.state();
    let manager = Arc::clone(manager_state.inner());

    match event.id.as_ref() {
        "backup" => {
            manager.trigger_backup(QuickActionType::Tray);
        }
        "apply" => {
            manager.trigger_apply(QuickActionType::Tray);
        }
        "quit" => {
            app.exit(0);
        }
        other => {
            info!(
                target: "rgsm::quick_action::tray",
                "Tray menu item clicked: {other}."
            );
            if other.starts_with("timer.") {
                if let Some(duration) = other
                    .split('.')
                    .next_back()
                    .and_then(|value| value.parse::<u32>().ok())
                {
                    manager.update_interval(duration);
                }
            }
        }
    }
}
