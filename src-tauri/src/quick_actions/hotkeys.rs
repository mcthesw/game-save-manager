use tauri::App;
use tauri::GlobalShortcutManager;

use super::*;
use crate::config::Config;
use crate::config::QuickActions;

pub fn setup_hotkeys(config: &Config, app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let mut manager = app.global_shortcut_manager();
    config
        .quick_action
        .hotkeys
        .iter()
        .for_each(|(hotkey, action)| {
            manager
                .register(hotkey, action_to_function(action))
                .expect("Cannot setup hotkeys");
        });
    Ok(())
}

fn action_to_function(action: &QuickActions) -> impl Fn() {
    match action {
        QuickActions::Apply => move || {
            tauri::async_runtime::spawn(async move {
                quick_apply(QuickActionType::Hotkey).await;
            });
        },
        QuickActions::Backup => move || {
            tauri::async_runtime::spawn(async move {
                quick_backup(QuickActionType::Hotkey).await;
            });
        },
    }
}
