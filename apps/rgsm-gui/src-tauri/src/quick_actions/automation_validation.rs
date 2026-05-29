use rgsm_core::config::{GameAutomationSettingsDraft, get_config};

use crate::process_util;

pub fn validate_game_automation_target(
    storage_key: &str,
    automation: &GameAutomationSettingsDraft,
) -> anyhow::Result<()> {
    let config = get_config()?;
    let game = config
        .games
        .iter()
        .find(|game| game.storage_key == storage_key || game.name == storage_key)
        .ok_or_else(|| anyhow::anyhow!("Game with storage_key '{storage_key}' not found"))?;

    process_util::validate_process_target(game, automation)
}
