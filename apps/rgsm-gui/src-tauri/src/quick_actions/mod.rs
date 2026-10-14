mod auto_backup;
mod automation_validation;
mod hotkeys;
mod manager;
mod process_monitor;
mod scheduler;
mod tray;
mod utils;

pub use auto_backup::perform_changed_auto_backup;
pub use automation_validation::validate_game_automation_target;
pub use manager::QuickActionManager;
pub use process_monitor::ProcessMonitor;
pub use scheduler::{AutoBackupGameStatus, AutoBackupScheduler};
pub use utils::{
    QuickActionCompleted, QuickActionOperation, QuickActionStatus, QuickActionType,
    emit_quick_action_event, notify_backup_failed, notify_backup_skipped_unchanged, quick_apply,
    quick_backup, should_show_auto_backup_notification,
};

use std::sync::Arc;

use hotkeys::{RegisteredHotkeys, apply_hotkeys};
use tauri::{AppHandle, Manager};
use tray::setup_tray;

use rgsm_core::config::get_config;

pub fn setup(app: &mut tauri::App, desktop_enabled: bool) -> anyhow::Result<()> {
    let manager = QuickActionManager::new(app.handle());
    app.manage(Arc::clone(&manager));

    let scheduler = AutoBackupScheduler::spawn(app.handle().clone());
    scheduler.sync_from_config();
    app.manage(scheduler);

    let process_monitor = ProcessMonitor::spawn(app.handle().clone());
    process_monitor.sync_from_config();
    app.manage(process_monitor);

    if desktop_enabled {
        let config = get_config()?;
        setup_tray(app)?;
        // Global shortcuts are system-wide, so only desktop hosts own any.
        let registered_hotkeys = RegisteredHotkeys::default();
        if let Err(error) = apply_hotkeys(app.handle(), &config, &registered_hotkeys, &manager) {
            log::warn!("Could not register quick action hotkeys: {error:#}");
        }
        app.manage(registered_hotkeys);
    }
    Ok(())
}

/// Re-registers hotkeys after the configuration changed; a no-op on HTTP-only
/// hosts, which never register any.
pub fn refresh_hotkeys(app: &AppHandle) -> anyhow::Result<()> {
    apply_config_hotkeys(app, &get_config()?)
}

pub fn apply_config_hotkeys(
    app: &AppHandle,
    config: &rgsm_core::config::Config,
) -> anyhow::Result<()> {
    let Some(registered) = app.try_state::<RegisteredHotkeys>() else {
        return Ok(());
    };
    let Some(manager) = app.try_state::<Arc<QuickActionManager>>() else {
        return Ok(());
    };
    apply_hotkeys(app, config, &registered, &manager)
}
