use log::info;
use tauri::App;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::{
    config::Config,
    quick_actions::utils::{QuickActionType, quick_apply, quick_backup},
};

pub fn setup_hotkeys(config: &Config, app: &mut App) -> anyhow::Result<()> {
    info!(target:"rgsm::quick_action::hotkeys", "Setting up hotkeys");

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
        let apply_shortcut = Shortcut::try_from(apply_keys.join("+"))?;
        app.global_shortcut()
            .on_shortcut(apply_shortcut, |_app, _shortcut, event| {
                if event.state() == ShortcutState::Released {
                    info!(target:"rgsm::quick_action::hotkeys", "Apply hotkey pressed");
                    tauri::async_runtime::spawn(async move {
                        quick_apply(QuickActionType::Hotkey).await;
                    });
                };
            })?;
    }

    if !backup_keys.is_empty() {
        info!(
            target:"rgsm::quick_action::hotkeys",
            "Registering backup hotkey: {}", backup_keys.join("+")
        );
        let backup_shortcut = Shortcut::try_from(backup_keys.join("+"))?;
        app.global_shortcut()
            .on_shortcut(backup_shortcut, |_app, _shortcut, event| {
                if event.state() == ShortcutState::Released {
                    info!(target:"rgsm::quick_action::hotkeys", "Backup hotkey pressed");
                    tauri::async_runtime::spawn(async move {
                        quick_backup(QuickActionType::Hotkey).await;
                    });
                };
            })?;
    }
    info!(target:"rgsm::quick_action::hotkeys","All hotkey are registered.");
    Ok(())
}
