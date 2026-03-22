use std::collections::HashMap;

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
    Merged,
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
    GameSnapshots::new(game_name)
}

fn current_device_head(info: &GameSnapshots) -> Option<String> {
    info.current_device_head_cloned()
}

fn is_ancestor(
    descendant: &str,
    ancestor: &str,
    parent_map: &HashMap<String, Option<String>>,
) -> bool {
    let mut current = descendant.to_string();
    let mut visited = std::collections::HashSet::new();

    loop {
        if current == ancestor {
            return true;
        }
        if !visited.insert(current.clone()) {
            return false;
        }
        match parent_map.get(&current).cloned().flatten() {
            Some(parent) => current = parent,
            None => return false,
        }
    }
}

fn merge_snapshot(
    existing: &mut crate::backup::Snapshot,
    incoming: &crate::backup::Snapshot,
) -> Result<(), BackendError> {
    if existing.parent != incoming.parent {
        return Err(BackendError::Unexpected(anyhow::anyhow!(
            "Snapshot '{}' has incompatible parents during coexist merge",
            existing.date
        )));
    }

    if existing.device_id.is_none() {
        existing.device_id = incoming.device_id.clone();
    }
    if existing.archive_hash.is_none() {
        existing.archive_hash = incoming.archive_hash.clone();
    }
    if existing.describe.is_empty() && !incoming.describe.is_empty() {
        existing.describe = incoming.describe.clone();
    }
    if existing.size == 0 {
        existing.size = incoming.size;
    }
    if existing.path.is_empty() && !incoming.path.is_empty() {
        existing.path = incoming.path.clone();
    }

    // If either side promoted the snapshot to non-Timer (e.g. user "Keep"),
    // preserve that across sync so cleanup won't delete it on the other device.
    if existing.created_by != incoming.created_by {
        use crate::backup::CreatedBy;
        // Timer is the only auto-deletable variant; any non-Timer wins.
        if existing.created_by == CreatedBy::Timer {
            existing.created_by = incoming.created_by.clone();
        }
    }

    Ok(())
}

fn merge_snapshots_metadata(
    local: &GameSnapshots,
    remote: &GameSnapshots,
) -> Result<GameSnapshots, BackendError> {
    let mut merged = GameSnapshots::new(&local.name);
    merged.sync_version = local.sync_version.max(remote.sync_version);
    merged.last_sync_device = local
        .last_sync_device
        .clone()
        .or_else(|| remote.last_sync_device.clone());
    merged.last_sync_timestamp = local
        .last_sync_timestamp
        .clone()
        .or_else(|| remote.last_sync_timestamp.clone());

    let mut snapshots_by_date: HashMap<String, crate::backup::Snapshot> = local
        .backups
        .iter()
        .cloned()
        .map(|snapshot| (snapshot.date.clone(), snapshot))
        .collect();
    for snapshot in &remote.backups {
        match snapshots_by_date.get_mut(&snapshot.date) {
            Some(existing) => merge_snapshot(existing, snapshot)?,
            None => {
                snapshots_by_date.insert(snapshot.date.clone(), snapshot.clone());
            }
        }
    }

    merged.backups = snapshots_by_date.into_values().collect();
    merged
        .backups
        .sort_by(|left, right| left.date.cmp(&right.date));
    merged.device_heads = local.device_heads.clone();

    let parent_map: HashMap<String, Option<String>> = merged
        .backups
        .iter()
        .map(|snapshot| (snapshot.date.clone(), snapshot.parent.clone()))
        .collect();
    for (device_id, remote_head) in &remote.device_heads {
        match merged.device_heads.get(device_id) {
            None => {
                merged
                    .device_heads
                    .insert(device_id.clone(), remote_head.clone());
            }
            Some(local_head) if local_head == remote_head => {}
            Some(local_head) => {
                if is_ancestor(local_head, remote_head, &parent_map) {
                    continue;
                }
                if is_ancestor(remote_head, local_head, &parent_map) {
                    merged
                        .device_heads
                        .insert(device_id.clone(), remote_head.clone());
                    continue;
                }
                return Err(BackendError::Unexpected(anyhow::anyhow!(
                    "Device '{}' has incompatible heads during coexist merge",
                    device_id
                )));
            }
        }
    }

    merged.normalize_heads();
    Ok(merged)
}

async fn copy_remote_snapshots_into_local(
    backup_root: &std::path::Path,
    info: &mut GameSnapshots,
) -> Result<(), BackendError> {
    let local_game_dir = backup_root.join(&info.name);
    fs::create_dir_all(&local_game_dir).await?;

    for snapshot in &mut info.backups {
        let source = std::path::PathBuf::from(&snapshot.path);
        let target = local_game_dir.join(format!("{}.zip", snapshot.date));
        if !target.exists() {
            fs::copy(&source, &target).await?;
        }
        snapshot.path = target.to_string_lossy().to_string();
    }

    Ok(())
}

async fn coexist_game_from_remote(
    session: &CloudSyncSessionConfig,
    game: &crate::backup::Game,
    op: &opendal::Operator,
) -> Result<GameSnapshots, BackendError> {
    let backup_root = get_backup_path()?;
    let stage_root = new_stage_root(&backup_root, "game-coexist-stage");
    if stage_root.exists() {
        let _ = fs::remove_dir_all(&stage_root).await;
    }
    fs::create_dir_all(&stage_root).await?;

    let result = async {
        let mut downloaded = stage_remote_game_download(
            op,
            &game.name,
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

        copy_remote_snapshots_into_local(&backup_root, &mut downloaded).await?;

        let local = game.get_game_snapshots_info()?;
        let merged = merge_snapshots_metadata(&local, &downloaded)?;
        game.set_game_snapshots_info(&merged)?;

        upload_game_data(op, &game.name, session.normalized_max_concurrency(), None)
            .await
            .map_err(|err| match err {
                SyncOperationError::Cancelled => {
                    BackendError::Unexpected(anyhow::anyhow!("Sync unexpectedly cancelled"))
                }
                SyncOperationError::Backend(inner) => inner,
            })
    }
    .await;

    if stage_root.exists() {
        let _ = fs::remove_dir_all(&stage_root).await;
    }

    result
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
            .and_then(|info| current_device_head(&info));
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
                    current_device_head(&info),
                    current_device_head(&info),
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
            current_device_head(&info),
            current_device_head(&info),
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
                current_device_head(&local),
                current_device_head(&remote),
                SyncResult::Success,
                PendingAction::None,
            );
            Ok(SyncGameOutcome::AlreadyInSync)
        }
        SyncRelation::CurrentDeviceAhead => {
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
                current_device_head(&uploaded),
                current_device_head(&uploaded),
                SyncResult::Success,
                PendingAction::None,
            );
            Ok(SyncGameOutcome::Uploaded)
        }
        SyncRelation::CurrentDeviceBehind => {
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
                current_device_head(&downloaded),
                current_device_head(&downloaded),
                SyncResult::Success,
                PendingAction::None,
            );
            Ok(SyncGameOutcome::Downloaded)
        }
        SyncRelation::SharedTreeDiverged | SyncRelation::ParallelBranches => {
            let merged = coexist_game_from_remote(&session, game, &op).await?;
            record_game_state(
                &session,
                game_name,
                current_device_head(&merged),
                current_device_head(&merged),
                SyncResult::Success,
                PendingAction::None,
            );
            Ok(SyncGameOutcome::Merged)
        }
        SyncRelation::IncompatibleState => {
            record_game_state(
                &session,
                game_name,
                current_device_head(&local),
                current_device_head(&remote),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::Snapshot;
    use crate::device::get_current_device_id;

    fn snapshot(date: &str, parent: Option<&str>, device_id: Option<&str>) -> Snapshot {
        Snapshot {
            date: date.to_string(),
            describe: String::new(),
            path: format!("/tmp/{date}.zip"),
            size: 0,
            parent: parent.map(str::to_string),
            archive_hash: None,
            device_id: device_id.map(str::to_string),
            created_by: Default::default(),
        }
    }

    fn snapshots(name: &str, backups: Vec<Snapshot>, heads: &[(&str, &str)]) -> GameSnapshots {
        let mut snapshots = GameSnapshots::new(name);
        snapshots.backups = backups;
        for (device_id, head) in heads {
            snapshots
                .device_heads
                .insert((*device_id).to_string(), (*head).to_string());
        }
        snapshots
    }

    #[test]
    fn merge_snapshots_metadata_keeps_parallel_device_heads() {
        let current_device = get_current_device_id().clone();
        let local = snapshots(
            "TestGame",
            vec![snapshot("2025-01-01_00-00-00", None, Some(&current_device))],
            &[(current_device.as_str(), "2025-01-01_00-00-00")],
        );
        let remote = snapshots(
            "TestGame",
            vec![snapshot("2025-01-02_00-00-00", None, Some("remote-device"))],
            &[("remote-device", "2025-01-02_00-00-00")],
        );

        let merged = merge_snapshots_metadata(&local, &remote).expect("parallel merge should work");

        assert_eq!(merged.backups.len(), 2);
        assert_eq!(
            merged.head_for_device(&current_device).map(String::as_str),
            Some("2025-01-01_00-00-00")
        );
        assert_eq!(
            merged
                .head_for_device(&"remote-device".to_string())
                .map(String::as_str),
            Some("2025-01-02_00-00-00")
        );
    }

    #[test]
    fn merge_snapshots_metadata_rejects_incompatible_same_device_heads() {
        let device_id = get_current_device_id().clone();
        let local = snapshots(
            "TestGame",
            vec![snapshot("2025-01-01_00-00-00", None, Some(&device_id))],
            &[(device_id.as_str(), "2025-01-01_00-00-00")],
        );
        let remote = snapshots(
            "TestGame",
            vec![snapshot("2025-01-02_00-00-00", None, Some(&device_id))],
            &[(device_id.as_str(), "2025-01-02_00-00-00")],
        );

        assert!(merge_snapshots_metadata(&local, &remote).is_err());
    }
}
