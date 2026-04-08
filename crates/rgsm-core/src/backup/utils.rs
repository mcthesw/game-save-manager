use crate::backup::archive::RestoreNotifier;
use crate::config::{get_backup_path, get_config, set_config_local};
use crate::preclude::*;

use log::{error, info};
use std::fs;
use std::sync::Arc;
use tokio::sync::Semaphore;

use super::game::SnapshotCreated;
use super::storage_key::generate_unique_storage_key;
use super::{GameDraft, GameSnapshots};

async fn create_backup_folder(dir_name: &str) -> Result<(), BackupError> {
    let backup_path = get_backup_path()?.join(dir_name);
    let info: GameSnapshots = if !backup_path.exists() {
        fs::create_dir_all(&backup_path)?;
        GameSnapshots::new(dir_name)
    } else {
        // 如果已经存在，info从原来的文件中读取
        let bytes = fs::read(backup_path.join("Backups.json"));
        serde_json::from_slice(&bytes?)?
    };
    fs::write(
        backup_path.join("Backups.json"),
        serde_json::to_string_pretty(&info)?,
    )?;

    Ok(())
}

pub async fn create_game_backup(game: &GameDraft) -> Result<(), BackupError> {
    let mut config = get_config()?;

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
    create_backup_folder(&storage_key).await?;
    let mut new_game = game.clone().into_game(None);
    new_game.storage_key = storage_key;
    config.games.push(new_game);

    set_config_local(&config)?;
    Ok(())
}

pub async fn backup_all() -> Result<Vec<SnapshotCreated>, BackupError> {
    let config = get_config()?;
    if config.games.is_empty() {
        return Ok(Vec::new());
    }

    // Cap concurrent disk operations to avoid overwhelming the I/O subsystem.
    const MAX_CONCURRENT_BACKUPS: usize = 4;
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_BACKUPS));
    let mut set: tokio::task::JoinSet<Result<SnapshotCreated, BackupError>> =
        tokio::task::JoinSet::new();

    for game in config.games {
        let sem = Arc::clone(&semaphore);
        set.spawn(async move {
            let game_name = game.name.clone();
            let _permit = match sem.acquire_owned().await {
                Ok(permit) => permit,
                Err(e) => {
                    let msg = format!(
                        "Backup all semaphore closed while starting backup for game {game_name}: {e:?}"
                    );
                    error!(target: "rgsm::backup", "{msg}");
                    return Err(BackupError::Unexpected(anyhow::anyhow!(msg)));
                }
            };
            info!(target: "rgsm::backup", "Backup all: starting {}", game.name);
            game.create_snapshot("Backup all").await
        });
    }

    let mut created_snapshots = Vec::new();
    let mut first_error: Option<BackupError> = None;

    while let Some(join_result) = set.join_next().await {
        match join_result {
            Ok(Ok(created)) => {
                info!(target: "rgsm::backup", "Backup all succeeded for game {}", created.snapshots.name);
                created_snapshots.push(created);
            }
            Ok(Err(e)) => {
                error!(target: "rgsm::backup", "Backup all failed: {e:?}");
                first_error.get_or_insert(e);
            }
            Err(e) => {
                let panic_msg = format!("Backup all task panicked or was cancelled: {e:?}");
                error!(target: "rgsm::backup", "{panic_msg}");
                set.abort_all();
                first_error.get_or_insert(BackupError::Unexpected(anyhow::anyhow!(panic_msg)));
                break;
            }
        }
    }

    if let Some(e) = first_error {
        return Err(e);
    }

    Ok(created_snapshots)
}

#[allow(dead_code)]
pub async fn apply_all(
    notifier: Option<&dyn RestoreNotifier>,
) -> Result<Vec<GameSnapshots>, BackupError> {
    let config = get_config()?;
    let mut restored = Vec::new();
    for game in &config.games {
        let date = game
            .get_game_snapshots_info()?
            .backups
            .last()
            .ok_or(BackupError::NoBackupAvailable)?
            .date
            .clone();
        match game.restore_snapshot(&date, notifier) {
            Ok(snapshots) => {
                info!(target: "rgsm::backup", "Apply all succeeded for game {:#?} with date {}", game.name, date);
                restored.push(snapshots);
            }
            Err(e) => {
                error!(target: "rgsm::backup", "Apply all failed for game {:#?} with date {}", game, date);
                return Err(e);
            }
        }
    }
    Ok(restored)
}
