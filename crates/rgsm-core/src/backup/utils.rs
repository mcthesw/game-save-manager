use crate::preclude::*;

use std::fs;

use super::storage_key::generate_unique_storage_key;
use super::{Game, GameDraft, GameSnapshots};

fn create_backup_folder(backup_root: &std::path::Path, dir_name: &str) -> Result<(), BackupError> {
    let backup_path = backup_root.join(dir_name);
    let info: GameSnapshots = if !backup_path.exists() {
        fs::create_dir_all(&backup_path)?;
        GameSnapshots::new(dir_name)
    } else {
        // 如果已经存在，info从原来的文件中读取
        let bytes = fs::read(backup_path.join("Backups.json"));
        serde_json::from_slice(&bytes?)?
    };
    let bytes = serde_json::to_vec_pretty(&info)?;
    crate::atomic_file::write_bytes_atomically(&backup_path.join("Backups.json"), &bytes)?;

    Ok(())
}

pub fn create_game_backup(
    game: &GameDraft,
    config: &mut crate::config::Config,
) -> Result<Game, BackupError> {
    if config
        .games
        .iter()
        .any(|g| g.name.eq_ignore_ascii_case(&game.name))
    {
        return Err(BackupError::Unexpected(anyhow::anyhow!(
            "Game '{}' already exists; use update instead",
            game.name
        )));
    }

    let existing_keys: std::collections::HashSet<String> = config
        .games
        .iter()
        .filter(|g| !g.storage_key.is_empty())
        .map(|g| g.storage_key.clone())
        .collect();
    let storage_key = generate_unique_storage_key(&game.name, &existing_keys);
    create_backup_folder(
        &crate::config::resolve_backup_path(&config.backup_path),
        &storage_key,
    )?;
    let mut new_game = game.clone().into_game(None);
    new_game.storage_key = storage_key;
    config.games.push(new_game.clone());

    Ok(new_game)
}
