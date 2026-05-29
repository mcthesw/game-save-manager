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

use hotkeys::setup_hotkeys;
use tauri::Manager;
use tray::setup_tray;

use rgsm_core::config::get_config;

pub fn setup(app: &mut tauri::App) -> anyhow::Result<()> {
    let manager = QuickActionManager::new(app.handle());
    app.manage(manager);

    let scheduler = AutoBackupScheduler::spawn(app.handle().clone());
    scheduler.sync_from_config();
    app.manage(scheduler);

    let process_monitor = ProcessMonitor::spawn(app.handle().clone());
    process_monitor.sync_from_config();
    app.manage(process_monitor);

    let config = get_config()?;
    setup_tray(app)?;
    setup_hotkeys(&config, app)?;
    Ok(())
}
