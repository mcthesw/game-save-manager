use tauri::App;
use tauri::GlobalShortcutManager;

use super::*;
use crate::config::Config;

pub fn setup_hotkeys(config: &Config, app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let mut manager = app.global_shortcut_manager();

    let mut apply_keys = config.quick_action.hotkeys.apply.clone();
    apply_keys.retain(|x| !x.is_empty());
    if !config.quick_action.hotkeys.apply.is_empty() {
        let key_string = apply_keys.join("+");
        manager
            .register(&key_string, move || {
                tauri::async_runtime::spawn(async move {
                    quick_apply(QuickActionType::Hotkey).await;
                });
            })
            .expect("Cannot setup hotkeys");
    }

    let mut backup_keys = config.quick_action.hotkeys.backup.clone();
    backup_keys.retain(|x| !x.is_empty());
    if !config.quick_action.hotkeys.backup.is_empty() {
        let key_string = backup_keys.join("+");
        manager
            .register(&key_string, move || {
                tauri::async_runtime::spawn(async move {
                    quick_backup(QuickActionType::Hotkey).await;
                });
            })
            .expect("Cannot setup hotkeys");
    }
    Ok(())
}
