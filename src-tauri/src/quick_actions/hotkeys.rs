use log::info;
use tauri::App;

use crate::config::Config;

pub fn setup_hotkeys(config: &Config, app: &mut App) -> anyhow::Result<()> {
    info!(target:"rgsm::quick_action::hotkeys", "Setting up hotkeys");
    // TODO
    // let builder = tauri_plugin_global_shortcut::Builder::new();
    // let mut manager = app.global_shortcut_manager();

    // let mut apply_keys = config.quick_action.hotkeys.apply.clone();
    // apply_keys.retain(|x| !x.is_empty());
    // let key_string = apply_keys.join("+");
    // if !key_string.is_empty() {
    //     info!(
    //         target:"rgsm::quick_action::hotkeys",
    //         "Registering apply hotkey: {}", key_string
    //     );
    //     manager
    //         .register(&key_string, move || {
    //             tauri::async_runtime::spawn(async move {
    //                 quick_apply(QuickActionType::Hotkey).await;
    //             });
    //         })
    //         .expect("Cannot setup hotkeys");
    // }

    // let mut backup_keys = config.quick_action.hotkeys.backup.clone();
    // backup_keys.retain(|x| !x.is_empty());
    // let key_string = backup_keys.join("+");
    // if !key_string.is_empty() {
    //     info!(
    //         target:"rgsm::quick_action::hotkeys",
    //         "Registering backup hotkey: {}", key_string
    //     );
    //     manager
    //         .register(&key_string, move || {
    //             tauri::async_runtime::spawn(async move {
    //                 quick_backup(QuickActionType::Hotkey).await;
    //             });
    //         })
    //         .expect("Cannot setup hotkeys");
    // }

    info!(target:"rgsm::quick_action::hotkeys","All hotkey are registered.");
    Ok(())
}
