mod hotkeys;
mod timer;
mod tray;
mod utils;

use utils::*;

use hotkeys::setup_hotkeys;
use timer::setup_timer;
pub use timer::AutoBackupDuration;
use tray::setup_tray;
pub use utils::set_current_game;

use crate::config::get_config;

pub fn setup(app: &mut tauri::App) -> anyhow::Result<()> {
    let config = get_config()?;
    setup_tray(app)?;
    setup_timer(app)?;
    setup_hotkeys(&config, app)?;
    Ok(())
}
