use std::{fmt::Write, sync::Arc};

use log::{error, info};
use tauri::{
    AppHandle, Manager, State,
    menu::{MenuBuilder, MenuEvent, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
};

use super::{QuickActionManager, QuickActionType};
use rgsm_core::backup::Game;

use rust_i18n::t;

const GAME_ITEM_ID: &str = "game";

pub fn setup_tray(app: &mut tauri::App) -> anyhow::Result<()> {
    info!(target: "rgsm::quick_action::tray", "Setting up tray icon");

    let manager_state: State<Arc<QuickActionManager>> = app.state();
    let manager = Arc::clone(manager_state.inner());

    let current_game_label = manager
        .current_game()
        .map(|game| game.name)
        .unwrap_or_else(|| t!("backend.tray.no_game_selected").into());

    let current_quick_action_game = MenuItemBuilder::new(current_game_label)
        .id(GAME_ITEM_ID)
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

    manager.register_tray_game_item(current_quick_action_game);

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
        // The label names the game quick backup acts on, so it opens that game;
        // without a selection it opens the app to pick one.
        GAME_ITEM_ID => {
            let shown = match manager.current_game() {
                Some(game) => {
                    crate::main_window::show_main_window_at(app, &game_management_route(&game))
                }
                None => crate::main_window::show_main_window(app),
            };
            if let Err(error) = shown {
                error!(target: "rgsm::quick_action::tray", "Cannot show main window: {error}");
            }
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

/// Route of a game's management page, encoded like `getGameManagementPath` in
/// the frontend so the window resolves the same game.
fn game_management_route(game: &Game) -> String {
    let mut route = format!("/Management/{}", encode_uri_component(&game.name));
    if !game.storage_key.is_empty() {
        let _ = write!(route, "?gameId={}", encode_uri_component(&game.storage_key));
    }
    route
}

/// Percent-encodes a value the way JavaScript's `encodeURIComponent` does.
fn encode_uri_component(value: &str) -> String {
    const UNRESERVED_MARKS: &[u8] = b"-_.!~*'()";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || UNRESERVED_MARKS.contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::game_management_route;
    use rgsm_core::backup::Game;

    fn game(name: &str, storage_key: &str) -> Game {
        serde_json::from_value(serde_json::json!({
            "name": name,
            "storage_key": storage_key,
            "save_paths": [],
        }))
        .expect("test game is valid")
    }

    // Expected routes come from the frontend's `getGameManagementPath`.
    #[test]
    fn management_routes_match_the_frontend_encoding() {
        assert_eq!(
            game_management_route(&game("Save %2F / ? #", "key&one")),
            "/Management/Save%20%252F%20%2F%20%3F%20%23?gameId=key%26one"
        );
        assert_eq!(
            game_management_route(&game("Baldur's Gate (3)!", "bg3")),
            "/Management/Baldur's%20Gate%20(3)!?gameId=bg3"
        );
        assert_eq!(
            game_management_route(&game("空洞骑士", "")),
            "/Management/%E7%A9%BA%E6%B4%9E%E9%AA%91%E5%A3%AB"
        );
    }
}
