use std::sync::Arc;

use log::info;
use tauri::{App, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::{
    config::Config,
    quick_actions::{QuickActionManager, QuickActionType},
};

pub fn setup_hotkeys(config: &Config, app: &mut App) -> anyhow::Result<()> {
    info!(target:"rgsm::quick_action::hotkeys", "Setting up hotkeys");

    let manager_state: tauri::State<Arc<QuickActionManager>> = app.state();
    let manager = Arc::clone(manager_state.inner());

    let apply_keys = config
        .quick_action
        .hotkeys
        .apply
        .clone()
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>();
    let backup_keys = config
        .quick_action
        .hotkeys
        .backup
        .clone()
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>();

    if !apply_keys.is_empty() {
        info!(
            target:"rgsm::quick_action::hotkeys",
            "Registering apply hotkey: {}", apply_keys.join("+")
        );
        let apply_manager = Arc::clone(&manager);
        let apply_shortcut = Shortcut::try_from(apply_keys.join("+"))?;
        app.global_shortcut()
            .on_shortcut(apply_shortcut, move |_app, _shortcut, event| {
                if event.state() == ShortcutState::Released {
                    info!(target:"rgsm::quick_action::hotkeys", "Apply hotkey pressed");
                    apply_manager.trigger_apply(QuickActionType::Hotkey);
                }
            })?;
    }

    if !backup_keys.is_empty() {
        info!(
            target:"rgsm::quick_action::hotkeys",
            "Registering backup hotkey: {}", backup_keys.join("+")
        );
        let backup_manager = Arc::clone(&manager);
        let backup_shortcut = Shortcut::try_from(backup_keys.join("+"))?;
        app.global_shortcut()
            .on_shortcut(backup_shortcut, move |_app, _shortcut, event| {
                if event.state() == ShortcutState::Released {
                    info!(target:"rgsm::quick_action::hotkeys", "Backup hotkey pressed");
                    backup_manager.trigger_backup(QuickActionType::Hotkey);
                }
            })?;
    }
    info!(target:"rgsm::quick_action::hotkeys","All hotkey are registered.");
    Ok(())
}
