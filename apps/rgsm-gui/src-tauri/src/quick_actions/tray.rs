use std::sync::Arc;

use log::{error, info};
use tauri::{
    AppHandle, Manager, State,
    menu::{MenuBuilder, MenuEvent, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
};

use super::{QuickActionManager, QuickActionType};

use rust_i18n::t;

pub fn setup_tray(app: &mut tauri::App) -> anyhow::Result<()> {
    info!(target: "rgsm::quick_action::tray", "Setting up tray icon");

    let manager_state: State<Arc<QuickActionManager>> = app.state();
    let manager = Arc::clone(manager_state.inner());

    let current_game_label = manager
        .current_game()
        .map(|game| game.name)
        .unwrap_or_else(|| t!("backend.tray.no_game_selected").into());

    let current_quick_action_game = MenuItemBuilder::new(current_game_label)
        .id("game")
        .enabled(true)
        .build(app)?;

    let tray_menu = MenuBuilder::new(app)
        .items(&[
            &current_quick_action_game,
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

    manager.register_tray_items(current_quick_action_game.clone());

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
        if let Err(error) = crate::main_window::show_main_window(app) {
            error!(target: "rgsm::quick_action::tray", "Cannot show main window: {error}");
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
        }
    }
}
