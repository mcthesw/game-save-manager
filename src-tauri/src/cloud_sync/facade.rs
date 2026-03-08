use log::warn;
use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::fs;
use tokio_util::sync::CancellationToken;

use super::conflict::{SyncRelation, determine_sync_relation};
use super::sync_state::{
    PendingAction, SyncResult, build_game_sync_state, update_config_sync_state,
    update_game_sync_state, with_sync_state,
};
use super::utils::{
    SyncOperationError, commit_staged_backup_root, load_remote_config, load_remote_game_snapshots,
    new_stage_root, replace_local_game_from_stage, stage_remote_game_download,
    target_backup_root_from_config, upload_config, upload_game_data,
};
use super::{Backend, CloudSyncSessionConfig};
use crate::backup::GameSnapshots;
use crate::config::{get_backup_path, get_config};
use crate::preclude::BackendError;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum SyncGameOutcome {
    AlreadyInSync,
    Uploaded,
    Downloaded,
    Conflict,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum BatchSyncItemStatus {
    Success,
    Cancelled,
    Failed(String),
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct BatchSyncItemReport {
    pub name: String,
    pub status: BatchSyncItemStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct BatchSyncReport {
    pub config: BatchSyncItemReport,
    pub games: Vec<BatchSyncItemReport>,
}

fn empty_batch_report() -> BatchSyncReport {
    BatchSyncReport {
        config: BatchSyncItemReport {
            name: "config".to_string(),
            status: BatchSyncItemStatus::Success,
        },
        games: Vec::new(),
    }
}

fn record_config_state(
    session: &CloudSyncSessionConfig,
    result: SyncResult,
    pending: PendingAction,
) {
    let state = build_game_sync_state(None, None, result, pending);
    if let Err(err) = with_sync_state(|sync_state| {
        update_config_sync_state(sync_state, session, state);
    }) {
        warn!("Failed to record config sync state: {err}");
    }
}

fn record_game_state(
    session: &CloudSyncSessionConfig,
    game_name: &str,
    local_head: Option<String>,
    remote_head: Option<String>,
    result: SyncResult,
    pending: PendingAction,
) {
    let state = build_game_sync_state(local_head, remote_head, result, pending);
    let game_name = game_name.to_string();
    if let Err(err) = with_sync_state(|sync_state| {
        update_game_sync_state(sync_state, session, &game_name, state);
    }) {
        warn!("Failed to record sync state for {game_name}: {err}");
    }
}

async fn upload_config_with_token(
    session: &CloudSyncSessionConfig,
    token: Option<&CancellationToken>,
) -> Result<(), SyncOperationError> {
    let op = session.get_op().map_err(SyncOperationError::Backend)?;
    if let Some(token) = token {
        tokio::select! {
            _ = token.cancelled() => Err(SyncOperationError::Cancelled),
            result = upload_config(&op) => result.map_err(SyncOperationError::Backend),
        }
    } else {
        upload_config(&op)
            .await
            .map_err(SyncOperationError::Backend)
    }
}

fn empty_remote_snapshots(game_name: &str) -> GameSnapshots {
    GameSnapshots {
        name: game_name.to_string(),
        backups: Vec::new(),
        head: None,
        sync_version: 0,
        last_sync_device: None,
        last_sync_timestamp: None,
    }
}

pub async fn upload_all_from_session(
    session: &CloudSyncSessionConfig,
    token: Option<CancellationToken>,
) -> Result<BatchSyncReport, BackendError> {
    let config = get_config()?;
    let op = session.get_op()?;
    let mut report = empty_batch_report();

    match upload_config_with_token(session, token.as_ref()).await {
        Ok(()) => {
            record_config_state(session, SyncResult::Success, PendingAction::None);
        }
        Err(SyncOperationError::Cancelled) => {
            report.config.status = BatchSyncItemStatus::Cancelled;
            record_config_state(session, SyncResult::Cancelled, PendingAction::None);
            return Ok(report);
        }
        Err(SyncOperationError::Backend(err)) => {
            let message = err.to_string();
            report.config.status = BatchSyncItemStatus::Failed(message.clone());
            record_config_state(
                session,
                SyncResult::Error(message),
                PendingAction::RetryRequired,
            );
            return Ok(report);
        }
    }

    for game in config.games {
        let local_head = game
            .get_game_snapshots_info()
            .ok()
            .and_then(|info| info.head);
        match upload_game_data(
            &op,
            &game.name,
            session.normalized_max_concurrency(),
            token.clone(),
        )
        .await
        {
            Ok(info) => {
                report.games.push(BatchSyncItemReport {
                    name: game.name.clone(),
                    status: BatchSyncItemStatus::Success,
                });
                record_game_state(
                    session,
                    &game.name,
                    info.head.clone(),
                    info.head,
                    SyncResult::Success,
                    PendingAction::None,
                );
            }
            Err(SyncOperationError::Cancelled) => {
                report.games.push(BatchSyncItemReport {
                    name: game.name.clone(),
                    status: BatchSyncItemStatus::Cancelled,
                });
                record_game_state(
                    session,
                    &game.name,
                    local_head,
                    None,
                    SyncResult::Cancelled,
                    PendingAction::None,
                );
                return Ok(report);
            }
            Err(SyncOperationError::Backend(err)) => {
                let message = err.to_string();
                report.games.push(BatchSyncItemReport {
                    name: game.name.clone(),
                    status: BatchSyncItemStatus::Failed(message.clone()),
                });
                record_game_state(
                    session,
                    &game.name,
                    local_head,
                    None,
                    SyncResult::Error(message),
                    PendingAction::RetryRequired,
                );
                return Ok(report);
            }
        }
    }

    Ok(report)
}

pub async fn download_all_from_session(
    session: &CloudSyncSessionConfig,
    token: Option<CancellationToken>,
) -> Result<BatchSyncReport, BackendError> {
    let op = session.get_op()?;
    let mut report = empty_batch_report();

    let remote_config = match load_remote_config(&op, token.as_ref()).await {
        Ok(config) => config,
        Err(SyncOperationError::Cancelled) => {
            report.config.status = BatchSyncItemStatus::Cancelled;
            record_config_state(session, SyncResult::Cancelled, PendingAction::None);
            return Ok(report);
        }
        Err(SyncOperationError::Backend(err)) => {
            let message = err.to_string();
            report.config.status = BatchSyncItemStatus::Failed(message.clone());
            record_config_state(
                session,
                SyncResult::Error(message),
                PendingAction::RetryRequired,
            );
            return Ok(report);
        }
    };

    let target_backup_root = target_backup_root_from_config(&remote_config);
    let stage_root = new_stage_root(&target_backup_root, "download-stage");
    if stage_root.exists() {
        let _ = fs::remove_dir_all(&stage_root).await;
    }
    fs::create_dir_all(&stage_root).await?;

    let mut staged_snapshots: Vec<GameSnapshots> = Vec::new();
    for game in &remote_config.games {
        match stage_remote_game_download(
            &op,
            &game.name,
            &stage_root,
            session.normalized_max_concurrency(),
            token.clone(),
        )
        .await
        {
            Ok(info) => staged_snapshots.push(info),
            Err(SyncOperationError::Cancelled) => {
                report.config.status = BatchSyncItemStatus::Cancelled;
                let _ = fs::remove_dir_all(&stage_root).await;
                record_config_state(session, SyncResult::Cancelled, PendingAction::None);
                return Ok(report);
            }
            Err(SyncOperationError::Backend(err)) => {
                let message = err.to_string();
                report.config.status = BatchSyncItemStatus::Failed(message.clone());
                report.games.push(BatchSyncItemReport {
                    name: game.name.clone(),
                    status: BatchSyncItemStatus::Failed(message.clone()),
                });
                let _ = fs::remove_dir_all(&stage_root).await;
                record_config_state(
                    session,
                    SyncResult::Error(message.clone()),
                    PendingAction::RetryRequired,
                );
                record_game_state(
                    session,
                    &game.name,
                    None,
                    None,
                    SyncResult::Error(message),
                    PendingAction::RetryRequired,
                );
                return Ok(report);
            }
        }
    }

    if let Err(err) =
        commit_staged_backup_root(&stage_root, &target_backup_root, &remote_config).await
    {
        let message = err.to_string();
        report.config.status = BatchSyncItemStatus::Failed(message.clone());
        record_config_state(
            session,
            SyncResult::Error(message),
            PendingAction::RetryRequired,
        );
        return Ok(report);
    }

    record_config_state(session, SyncResult::Success, PendingAction::None);
    for info in staged_snapshots {
        report.games.push(BatchSyncItemReport {
            name: info.name.clone(),
            status: BatchSyncItemStatus::Success,
        });
        record_game_state(
            session,
            &info.name,
            info.head.clone(),
            info.head,
            SyncResult::Success,
            PendingAction::None,
        );
    }

    Ok(report)
}

pub async fn sync_game_from_config(game_name: &str) -> Result<SyncGameOutcome, BackendError> {
    let config = get_config()?;
    let session = CloudSyncSessionConfig::from(&config.settings.cloud_settings);
    let op = session.get_op()?;
    let game = config
        .games
        .iter()
        .find(|game| game.name == game_name)
        .ok_or_else(|| {
            BackendError::Unexpected(anyhow::anyhow!("Game '{}' not found", game_name))
        })?;

    let local = game.get_game_snapshots_info()?;
    let remote = load_remote_game_snapshots(&op, game_name, None)
        .await
        .map_err(|err| match err {
            SyncOperationError::Cancelled => {
                BackendError::Unexpected(anyhow::anyhow!("Sync unexpectedly cancelled"))
            }
            SyncOperationError::Backend(inner) => inner,
        })?
        .unwrap_or_else(|| empty_remote_snapshots(game_name));

    match determine_sync_relation(&local, &remote) {
        SyncRelation::InSync => {
            record_game_state(
                &session,
                game_name,
                local.head.clone(),
                remote.head.clone(),
                SyncResult::Success,
                PendingAction::None,
            );
            Ok(SyncGameOutcome::AlreadyInSync)
        }
        SyncRelation::LocalAhead => {
            let uploaded =
                upload_game_data(&op, game_name, session.normalized_max_concurrency(), None)
                    .await
                    .map_err(|err| match err {
                        SyncOperationError::Cancelled => {
                            BackendError::Unexpected(anyhow::anyhow!("Sync unexpectedly cancelled"))
                        }
                        SyncOperationError::Backend(inner) => inner,
                    })?;
            record_game_state(
                &session,
                game_name,
                uploaded.head.clone(),
                uploaded.head,
                SyncResult::Success,
                PendingAction::None,
            );
            Ok(SyncGameOutcome::Uploaded)
        }
        SyncRelation::LocalBehind => {
            let backup_root = get_backup_path()?;
            let stage_root = new_stage_root(&backup_root, "game-download-stage");
            if stage_root.exists() {
                let _ = fs::remove_dir_all(&stage_root).await;
            }
            fs::create_dir_all(&stage_root).await?;
            let downloaded = stage_remote_game_download(
                &op,
                game_name,
                &stage_root,
                session.normalized_max_concurrency(),
                None,
            )
            .await
            .map_err(|err| match err {
                SyncOperationError::Cancelled => {
                    BackendError::Unexpected(anyhow::anyhow!("Sync unexpectedly cancelled"))
                }
                SyncOperationError::Backend(inner) => inner,
            })?;
            replace_local_game_from_stage(&stage_root, &backup_root, game_name).await?;
            if stage_root.exists() {
                let _ = fs::remove_dir_all(&stage_root).await;
            }
            record_game_state(
                &session,
                game_name,
                downloaded.head.clone(),
                downloaded.head,
                SyncResult::Success,
                PendingAction::None,
            );
            Ok(SyncGameOutcome::Downloaded)
        }
        SyncRelation::Diverged | SyncRelation::Conflict | SyncRelation::Unknown => {
            record_game_state(
                &session,
                game_name,
                local.head.clone(),
                remote.head.clone(),
                SyncResult::Conflict,
                PendingAction::UserDecisionRequired,
            );
            Ok(SyncGameOutcome::Conflict)
        }
    }
}

pub fn session_from_backend(backend: &Backend) -> Result<CloudSyncSessionConfig, BackendError> {
    let config = get_config()?;
    Ok(CloudSyncSessionConfig {
        root_path: config.settings.cloud_settings.root_path,
        max_concurrency: config.settings.cloud_settings.max_concurrency.max(1),
        backend: backend.clone(),
    })
}
