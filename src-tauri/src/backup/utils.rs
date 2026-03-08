use crate::config::{get_backup_path, get_config, set_config_local};
use crate::preclude::*;

use log::{error, info};
use std::fs;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::Semaphore;

use super::game::SnapshotCreated;
use super::{GameDraft, GameSnapshots};

async fn create_backup_folder(name: &str) -> Result<(), BackupError> {
    let backup_path = get_backup_path()?.join(name);
    let info: GameSnapshots = if !backup_path.exists() {
        fs::create_dir_all(&backup_path)?;
        GameSnapshots::new(name)
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
    create_backup_folder(&game.name).await?;

    // 查找是否存在与新游戏中的 `name` 字段相同的游戏
    let pos = config.games.iter().position(|g| g.name == game.name);
    match pos {
        Some(index) => {
            // 如果找到了，就用新的游戏覆盖它
            let existing = &config.games[index];
            config.games[index] = game.clone().into_game(Some(existing));
        }
        None => {
            // 如果没有找到，就将新的游戏添加到 `games` 数组中
            config.games.push(game.clone().into_game(None));
        }
    }
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
pub async fn apply_all(app_handle: Option<&AppHandle>) -> Result<Vec<GameSnapshots>, BackupError> {
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
        match game.restore_snapshot(&date, app_handle) {
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
