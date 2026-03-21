mod hotkeys;
mod manager;
mod scheduler;
mod tray;
mod utils;

pub use manager::QuickActionManager;
pub use scheduler::{AutoBackupGameStatus, AutoBackupScheduler};
pub use utils::{
    QuickActionCompleted, QuickActionOperation, QuickActionStatus, QuickActionType, quick_apply,
    quick_backup,
};

use hotkeys::setup_hotkeys;
use tauri::Manager;
use tray::setup_tray;

use crate::config::get_config;

pub fn setup(app: &mut tauri::App) -> anyhow::Result<()> {
    let manager = QuickActionManager::new(app.handle());
    app.manage(manager);

    let scheduler = AutoBackupScheduler::spawn(app.handle().clone());
    scheduler.sync_from_config();
    app.manage(scheduler);

    let config = get_config()?;
    setup_tray(app)?;
    setup_hotkeys(&config, app)?;
    Ok(())
}
