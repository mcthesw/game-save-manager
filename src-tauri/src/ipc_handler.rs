use crate::backup::{ExtraBackupItem, Game, GameDraft, GameSnapshots};
use crate::cloud_sync::{
    self, BatchSyncItemStatus, BatchSyncReport, CancelCloudSyncResult, CloudSyncSessionConfig,
    CloudSyncTaskManager, SyncGameOutcome,
};
use crate::config::{Config, QuickActionSoundPreferences, get_backup_path, get_config};
use crate::device::{Device, get_current_device_id};
use crate::hooks::{
    BeforeRestoreCtx, ConfigSavedCtx, GameAddedCtx, GameDeletedCtx, GameUpdatedCtx, HookPipeline,
    HookSource, MetadataChangedCtx, SnapshotAppliedCtx, SnapshotCreatedCtx, SnapshotDeletedCtx,
};
use crate::ludusavi_manifest::{self, ImportableGame, LudusaviManifestStatus, SavePath};
use crate::path_resolver;
use crate::preclude::*;
use crate::{backup, config, quick_actions, sound, system_fonts};

use anyhow::Result;
use log::{debug, error, info, warn};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, Window};
use tauri_plugin_dialog::DialogExt;
use tauri_specta::Event;

/// Typed error for restore operations, allowing the frontend to
/// pattern-match on specific failure modes without string parsing.
#[derive(Debug, Serialize, Deserialize, Clone, Type, thiserror::Error)]
#[serde(tag = "type")]
pub enum RestoreError {
    #[error("Integrity check failed: expected {expected}, got {actual}")]
    IntegrityCheckFailed { expected: String, actual: String },
    #[error("Backup not found: {date}")]
    BackupNotFound { date: String },
    #[error("Decompression failed: {message}")]
    DecompressFailed { message: String },
    #[error("IO error: {message}")]
    Io { message: String },
    #[error("{message}")]
    Other { message: String },
}

impl From<BackupError> for RestoreError {
    fn from(e: BackupError) -> Self {
        match e {
            BackupError::IntegrityCheckFailed { expected, actual } => {
                RestoreError::IntegrityCheckFailed { expected, actual }
            }
            BackupError::BackupNotExist { date, .. } => RestoreError::BackupNotFound { date },
            BackupError::Compress(ce) => RestoreError::DecompressFailed {
                message: ce.to_string(),
            },
            BackupError::Io(io) => RestoreError::Io {
                message: io.to_string(),
            },
            other => RestoreError::Other {
                message: other.to_string(),
            },
        }
    }
}

fn batch_report_failed_item(report: &BatchSyncReport) -> Option<String> {
    let mut items = Vec::with_capacity(report.games.len() + 1);
    items.push(&report.config.status);
    items.extend(report.games.iter().map(|item| &item.status));
    items.into_iter().find_map(|status| match status {
        BatchSyncItemStatus::Failed(message) => Some(message.clone()),
        _ => None,
    })
}

fn summarize_batch_result(
    result: &Result<BatchSyncReport, BackendError>,
) -> (cloud_sync::CloudSyncJobStatus, Option<String>) {
    match result {
        Ok(report) => {
            if let Some(message) = batch_report_failed_item(report) {
                (cloud_sync::CloudSyncJobStatus::Failed, Some(message))
            } else if matches!(report.config.status, BatchSyncItemStatus::Cancelled)
                || report
                    .games
                    .iter()
                    .any(|item| matches!(item.status, BatchSyncItemStatus::Cancelled))
            {
                (cloud_sync::CloudSyncJobStatus::Cancelled, None)
            } else {
                (cloud_sync::CloudSyncJobStatus::Completed, None)
            }
        }
        Err(err) => (
            cloud_sync::CloudSyncJobStatus::Failed,
            Some(err.to_string()),
        ),
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub enum NotificationLevel {
    info,
    warning,
    error,
}
#[derive(Debug, Serialize, Deserialize, Clone, Type, Event)]
pub struct IpcNotification {
    pub level: NotificationLevel,
    pub title: String,
    pub msg: String,
}

#[tauri::command]
#[specta::specta]
pub async fn open_url(url: String) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Opening url: {}", url);
    open::that(url).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to open url: {:?}", e);
        e.to_string()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn open_file_or_folder(path: String) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Opening file or folder: {}", path);

    let config = get_config().map_err(|e| e.to_string())?;
    let path = path_resolver::resolve_path(&path, None, &config).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to resolve url: {:?}", e);
        e.to_string()
    })?;

    debug!(target:"rgsm::ipc", "Resolved url: {}", path.display());
    open::that(path).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to open file or folder: {:?}", e);
        e.to_string()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn get_app_log_dir(app: AppHandle) -> Result<String, String> {
    info!(target:"rgsm::ipc", "Getting app log directory");

    let log_dir = app.path().app_log_dir().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get app log directory: {:?}", e);
        e.to_string()
    })?;

    debug!(target:"rgsm::ipc", "Log directory: {}", log_dir.display());
    Ok(log_dir.to_string_lossy().to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn choose_save_file(app: AppHandle) -> Result<String, String> {
    info!(target:"rgsm::ipc", "Opening file dialog.");
    if let Some(path) = app.dialog().file().blocking_pick_file() {
        info!(target:"rgsm::ipc","Successfully picked file: {:#?}",path);
        Ok(path.to_string())
    } else {
        warn!(target:"rgsm::ipc", "Failed to open dialog or user close the dialog.");
        Err("Failed to open dialog.".to_string())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn choose_save_dir(app: AppHandle) -> Result<String, String> {
    info!(target:"rgsm::ipc","Opening folder dialog.");
    if let Some(path) = app.dialog().file().blocking_pick_folder() {
        info!(target:"rgsm::ipc","Successfully picked folder: {:#?}",path);
        Ok(path.to_string())
    } else {
        warn!(target:"rgsm::ipc", "Failed to open dialog or user close the dialog.");
        Err("Failed to open dialog.".to_string())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn get_local_config() -> Result<Config, String> {
    info!(target:"rgsm::ipc", "Getting local config.");
    get_config().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn add_game(game: GameDraft, app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Adding game draft: {:?}", game);
    let previous_game = get_config()
        .ok()
        .and_then(|config| config.games.iter().find(|g| g.name == game.name).cloned());

    backup::create_game_backup(&game).await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to add game: {:?}", e);
        e.to_string()
    })?;

    if let Ok(config) = get_config() {
        if let Some(saved_game) = config.games.iter().find(|g| g.name == game.name) {
            let pipeline = app_handle.state::<Arc<HookPipeline>>();
            if let Some(previous_game) = previous_game {
                pipeline
                    .fire_game_updated(&GameUpdatedCtx {
                        source: HookSource::UserManual,
                        previous_game,
                        game: saved_game.clone(),
                    })
                    .await;
            } else if let Ok(snapshots) = saved_game.get_game_snapshots_info() {
                pipeline
                    .fire_game_added(&GameAddedCtx {
                        source: HookSource::UserManual,
                        game: saved_game.clone(),
                        snapshots,
                    })
                    .await;
            }
        }
    }

    info!(target:"rgsm::ipc", "Successfully added game draft: {:?}", game.name);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn restore_snapshot(
    game: Game,
    date: String,
    app: AppHandle,
) -> Result<(), RestoreError> {
    info!(target:"rgsm::ipc", "Applying backup: {:?} for game: {:?}", date, game);

    // Build gate context and run pre-restore hooks (extra backup, integrity check).
    let pre_snapshots = game.get_game_snapshots_info().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to read snapshots: {:?}", e);
        RestoreError::from(e)
    })?;
    let snapshot = pre_snapshots
        .backups
        .iter()
        .find(|s| s.date == date)
        .cloned()
        .ok_or_else(|| RestoreError::Other {
            message: format!("Snapshot {date} not found"),
        })?;

    let archive_path = get_backup_path()
        .map_err(|e| RestoreError::Other {
            message: e.to_string(),
        })?
        .join(&game.name)
        .join(format!("{date}.zip"));

    {
        let pipeline = app.state::<Arc<HookPipeline>>();
        pipeline
            .fire_before_restore(&BeforeRestoreCtx {
                source: HookSource::UserManual,
                game: game.clone(),
                snapshot: snapshot.clone(),
                snapshots: pre_snapshots,
                archive_path,
            })
            .await
            .map_err(RestoreError::from)?;
    }

    // Core restore: decompress + update HEAD.
    let snapshots = game.restore_snapshot(&date, Some(&app)).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to apply backup: {:?}", e);
        RestoreError::from(e)
    })?;

    {
        let pipeline = app.state::<Arc<HookPipeline>>();
        pipeline
            .fire_snapshot_applied(&SnapshotAppliedCtx {
                source: HookSource::UserManual,
                game: game.clone(),
                snapshot,
                snapshots,
            })
            .await;
    }

    info!(target:"rgsm::ipc", "Successfully applied backup: {:?} for game: {:?}", date, game);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_snapshot(
    game: Game,
    date: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Deleting backup: {:?} for game: {:?}", date, game);
    let deleted = game.delete_snapshot(&date).await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to delete backup: {:?}", e);
        e.to_string()
    })?;

    {
        let pipeline = app_handle.state::<Arc<HookPipeline>>();
        pipeline
            .fire_snapshot_deleted(&SnapshotDeletedCtx {
                source: HookSource::UserManual,
                game: game.clone(),
                snapshots: deleted.snapshots,
                deleted_remote_paths: vec![deleted.remote_zip_path],
            })
            .await;
    }

    info!(target:"rgsm::ipc", "Successfully deleted backup: {:?} for game: {:?}", date, game);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn batch_delete_snapshots(
    game: Game,
    dates: Vec<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Batch deleting {} snapshots for game: {:?}", dates.len(), game.name);
    let deleted = game.batch_delete_snapshots(&dates).await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to batch delete snapshots: {:?}", e);
        e.to_string()
    })?;

    {
        let pipeline = app_handle.state::<Arc<HookPipeline>>();
        pipeline
            .fire_snapshot_deleted(&SnapshotDeletedCtx {
                source: HookSource::UserManual,
                game: game.clone(),
                snapshots: deleted.snapshots,
                deleted_remote_paths: deleted.deleted_remote_paths,
            })
            .await;
    }

    info!(target:"rgsm::ipc", "Successfully batch deleted {} snapshots for game: {:?}", dates.len(), game.name);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_game(game: Game, app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Deleting game: {:?}", game);
    let deleted = game.delete_game().await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to delete game: {:?}", e);
        e.to_string()
    })?;

    {
        let pipeline = app_handle.state::<Arc<HookPipeline>>();
        pipeline
            .fire_game_deleted(&GameDeletedCtx {
                source: HookSource::UserManual,
                game_name: game.name.clone(),
                remote_game_dir_path: deleted.remote_game_dir_path,
            })
            .await;
    }

    info!(target:"rgsm::ipc", "Successfully deleted game: {:?}", game);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_game_snapshots_info(game: Game) -> Result<GameSnapshots, String> {
    info!(target:"rgsm::ipc", "Getting backup list info for game: {:?}", game);
    game.get_game_snapshots_info().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get backup list info: {:?}", e);
        e.to_string()
    })
}

/// Verify archive integrity by comparing the stored hash against a freshly computed one.
/// Returns `true` if the hash matches (or no stored hash exists), `false` if mismatched.
#[tauri::command]
#[specta::specta]
pub async fn verify_archive_integrity(
    archive_path: String,
    expected_hash: Option<String>,
) -> Result<bool, String> {
    use crate::backup::compute_file_hash;

    let Some(expected) = expected_hash else {
        return Ok(true);
    };
    let actual = compute_file_hash(std::path::Path::new(&archive_path)).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to compute archive hash: {:?}", e);
        e.to_string()
    })?;
    Ok(actual == expected)
}

#[tauri::command]
#[specta::specta]
pub async fn set_config(app_handle: AppHandle, config: Config) -> Result<(), String> {
    debug!(target:"rgsm::ipc", "Setting config: {:?}", config.clone().sanitize());
    config::set_config(&config).await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to set config: {:?}", e);
        e.to_string()
    })?;
    let pipeline = app_handle.state::<Arc<HookPipeline>>();
    pipeline
        .fire_config_saved(&ConfigSavedCtx {
            source: HookSource::UserManual,
        })
        .await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn reset_settings() -> Result<(), String> {
    info!(target:"rgsm::ipc", "Resetting settings.");
    config::reset_settings().await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to reset settings: {:?}", e);
        e.to_string()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn create_snapshot(
    game: Game,
    describe: String,
    window: Window,
    app_handle: AppHandle,
) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Backing up save for game: {:?}", game);
    let created = handle_backup_err(game.create_snapshot(&describe).await, window)?;

    {
        let pipeline = app_handle.state::<Arc<HookPipeline>>();
        if let Some(snapshot) = created.snapshots.backups.last().cloned() {
            pipeline
                .fire_snapshot_created(&SnapshotCreatedCtx {
                    source: HookSource::UserManual,
                    game: game.clone(),
                    snapshot,
                    snapshots: created.snapshots,
                    local_zip_path: created.local_zip_path,
                    remote_zip_path: created.remote_zip_path,
                })
                .await;
        }
    }

    info!(target:"rgsm::ipc", "Successfully backed up save for game: {:?}", game);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn open_backup_folder(game: Game) -> Result<bool, String> {
    info!(target:"rgsm::ipc", "Opening backup folder for game: {:?}", game);
    let backup_path = get_backup_path().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get backup path: {:?}", e);
        e.to_string()
    })?;
    let p = backup_path.join(game.name);
    Ok(open::that(p).is_ok())
}

#[tauri::command]
#[specta::specta]
pub async fn get_game_extra_backups(game: Game) -> Result<Vec<ExtraBackupItem>, String> {
    info!(target:"rgsm::ipc", "Getting extra backups for game: {:?}", game);
    backup::list_extra_backups(&game).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to list extra backups: {:?}", e);
        e.to_string()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn delete_extra_backup(game: Game, date: String) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Deleting extra backup: {:?} for game: {:?}", date, game);
    backup::delete_extra_backup(&game, &date).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to delete extra backup: {:?}", e);
        e.to_string()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn restore_extra_backup(game: Game, date: String, app: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Restoring extra backup: {:?} for game: {:?}", date, game);
    backup::restore_extra_backup(&game, &date, Some(&app)).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to restore extra backup: {:?}", e);
        e.to_string()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn open_extra_backup_folder(game: Game) -> Result<bool, String> {
    info!(target:"rgsm::ipc", "Opening extra backup folder for game: {:?}", game);
    let p = backup::extra_backup_folder_path(&game).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get extra backup path: {:?}", e);
        e.to_string()
    })?;
    Ok(open::that(p).is_ok())
}

#[tauri::command]
#[specta::specta]
pub async fn check_cloud_backend(session: CloudSyncSessionConfig) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Checking cloud backend: {:?}", session.backend.clone().sanitize());
    match session.check().await {
        Ok(_) => {
            info!(target:"rgsm::ipc", "Successfully checked cloud backend: {:?}", session.backend.sanitize());
            Ok(())
        }
        Err(e) => {
            error!(target:"rgsm::ipc", "Failed to check cloud backend: {:?}", e);
            Err(e.to_string())
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_upload_all(
    session: CloudSyncSessionConfig,
    app_handle: AppHandle,
) -> Result<BatchSyncReport, String> {
    let manager_state: tauri::State<Arc<CloudSyncTaskManager>> = app_handle.state();
    let manager = Arc::clone(manager_state.inner());
    let _ = manager.cancel_all().await;

    let description = format!(
        "Overwrite upload ({:?})",
        session.backend.clone().sanitize()
    );
    let (job_id, token) = manager.begin_manual_job(description.clone()).await;
    let result = cloud_sync::upload_all_from_session(&session, Some(token)).await;
    let (status, error) = summarize_batch_result(&result);
    manager
        .finish_manual_job(job_id, &description, status, error.clone())
        .await;
    result.map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_download_all(
    session: CloudSyncSessionConfig,
    app_handle: AppHandle,
) -> Result<BatchSyncReport, String> {
    let manager_state: tauri::State<Arc<CloudSyncTaskManager>> = app_handle.state();
    let manager = Arc::clone(manager_state.inner());
    let _ = manager.cancel_all().await;

    let description = format!(
        "Overwrite download ({:?})",
        session.backend.clone().sanitize()
    );
    let (job_id, token) = manager.begin_manual_job(description.clone()).await;
    let result = cloud_sync::download_all_from_session(&session, Some(token)).await;
    let (status, error) = summarize_batch_result(&result);
    manager
        .finish_manual_job(job_id, &description, status, error.clone())
        .await;
    result.map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn set_snapshot_description(
    game: Game,
    date: String,
    describe: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Setting backup describe for game: {:?}", game);
    let snapshots = game
        .set_snapshot_description(&date, &describe)
        .await
        .map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to set backup describe: {:?}", e);
            e.to_string()
        })?;

    {
        let pipeline = app_handle.state::<Arc<HookPipeline>>();
        pipeline
            .fire_metadata_changed(&MetadataChangedCtx {
                source: HookSource::UserManual,
                game: game.clone(),
                snapshots,
            })
            .await;
    }

    info!(target:"rgsm::ipc", "Successfully set backup {} describe for game: {:?}", date,game);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn backup_all(app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::ipc","Backing up all games.");
    let created_snapshots = backup::backup_all().await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to backup all games: {:?}", e);
        e.to_string()
    })?;

    {
        let pipeline = app_handle.state::<Arc<HookPipeline>>();
        let config = get_config().ok();
        for created in created_snapshots {
            let game = config
                .as_ref()
                .and_then(|c| c.games.iter().find(|g| g.name == created.snapshots.name))
                .cloned();
            if let (Some(game), Some(snapshot)) = (game, created.snapshots.backups.last().cloned())
            {
                pipeline
                    .fire_snapshot_created(&SnapshotCreatedCtx {
                        source: HookSource::BatchOperation,
                        game,
                        snapshot,
                        snapshots: created.snapshots,
                        local_zip_path: created.local_zip_path,
                        remote_zip_path: created.remote_zip_path,
                    })
                    .await;
            }
        }
    }

    info!(target:"rgsm::ipc","Successfully backed up all games.");
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn apply_all(app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::ipc","Applying all backups.");
    let config = get_config().map_err(|e| e.to_string())?;
    let pipeline = app_handle.state::<Arc<HookPipeline>>();
    let backup_base = get_backup_path().map_err(|e| e.to_string())?;

    for game in &config.games {
        let snapshots_info = game.get_game_snapshots_info().map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to read snapshots for {}: {e:?}", game.name);
            e.to_string()
        })?;
        let Some(snapshot) = snapshots_info.backups.last().cloned() else {
            warn!(target:"rgsm::ipc", "No backups for {}, skipping", game.name);
            continue;
        };
        let archive_path = backup_base
            .join(&game.name)
            .join(format!("{}.zip", snapshot.date));

        // Gate: pre-restore hooks
        if let Err(e) = pipeline
            .fire_before_restore(&BeforeRestoreCtx {
                source: HookSource::BatchOperation,
                game: game.clone(),
                snapshot: snapshot.clone(),
                snapshots: snapshots_info,
                archive_path,
            })
            .await
        {
            error!(target:"rgsm::ipc", "Pre-restore hook failed for {}: {e:#}", game.name);
            return Err(e.to_string());
        }

        let snapshots = game
            .restore_snapshot(&snapshot.date, Some(&app_handle))
            .map_err(|e| {
                error!(target:"rgsm::ipc", "Apply all failed for {}: {e:?}", game.name);
                e.to_string()
            })?;

        pipeline
            .fire_snapshot_applied(&SnapshotAppliedCtx {
                source: HookSource::BatchOperation,
                game: game.clone(),
                snapshot,
                snapshots,
            })
            .await;
    }

    info!(target:"rgsm::ipc","Successfully applied all backups.");
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_quick_backup_game(app_handle: AppHandle, game: Game) -> Result<(), String> {
    info!(target:"rgsm::ipc","Setting quick backup game to: {:?}", game);
    let manager_state: tauri::State<Arc<quick_actions::QuickActionManager>> = app_handle.state();
    let manager = Arc::clone(manager_state.inner());
    manager
        .set_quick_backup_game(game.clone())
        .await
        .map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to set quick backup game: {:?}", e);
            e.to_string()
        })?;
    info!(target:"rgsm::ipc","Successfully set quick backup game to: {:?}", game);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_quick_action_sound_preview(
    app: AppHandle,
    preferences: QuickActionSoundPreferences,
    effect: sound::QuickActionSoundEffect,
) -> Result<(), String> {
    let manager = app.state::<sound::SoundManager>();
    manager
        .toggle_preview(preferences, effect)
        .await
        .map_err(|err| {
            error!(target: "rgsm::sound", "Failed to preview sound: {err:?}");
            err.to_string()
        })
}

#[tauri::command]
#[specta::specta]
pub async fn stop_sound_playback(app: AppHandle) -> Result<(), String> {
    let manager = app.state::<sound::SoundManager>();
    manager.stop().await.map_err(|err| {
        error!(target: "rgsm::sound", "Failed to stop sound: {err:?}");
        err.to_string()
    })
}

#[tauri::command]
#[specta::specta]
pub async fn choose_quick_action_sound_file(app: AppHandle) -> Result<String, String> {
    sound::choose_quick_action_sound_file(&app)
}

/// Resolves a path string containing variables to an actual filesystem path
///
/// This command allows the frontend to resolve paths with variables like <home>, <winAppData>, etc.
#[tauri::command]
#[specta::specta]
pub async fn resolve_path(path: String) -> Result<String, String> {
    info!(target:"rgsm::ipc", "Resolving path: {}", path);

    let config = get_config().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get config: {:?}", e);
        e.to_string()
    })?;

    let resolved_path = path_resolver::resolve_path(&path, None, &config).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to resolve path: {:?}", e);
        e.to_string()
    })?;

    let path_str = resolved_path.to_str().ok_or_else(|| {
        let err = "Failed to convert resolved path to string";
        error!(target:"rgsm::ipc", "{}", err);
        err.to_string()
    })?;

    info!(target:"rgsm::ipc", "Successfully resolved path: {} -> {}", path, path_str);
    Ok(path_str.to_string())
}

/// Returns the current device, if not found, returns a default device
#[tauri::command]
#[specta::specta]
pub async fn get_current_device_info() -> Result<Device, String> {
    info!(target:"rgsm::ipc", "Getting current device info");

    let device_id = get_current_device_id();
    let config = get_config().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get config: {:?}", e);
        e.to_string()
    })?;

    Ok(config.devices.get(device_id).cloned().unwrap_or_default())
}

/// Set the HEAD pointer to a specific snapshot
/// This changes which snapshot new snapshots will branch from
#[tauri::command]
#[specta::specta]
pub async fn set_snapshot_head(
    game: Game,
    date: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Setting HEAD to snapshot: {:?} for game: {:?}", date, game);

    let mut saves = game.get_game_snapshots_info().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get game snapshots info: {:?}", e);
        e.to_string()
    })?;

    // Verify the snapshot exists
    if !saves.backups.iter().any(|s| s.date == date) {
        return Err("Snapshot not found".to_string());
    }

    saves.head = Some(date.clone());
    game.set_game_snapshots_info(&saves).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to set game snapshots info: {:?}", e);
        e.to_string()
    })?;

    {
        let pipeline = app_handle.state::<Arc<HookPipeline>>();
        pipeline
            .fire_metadata_changed(&MetadataChangedCtx {
                source: HookSource::UserManual,
                game: game.clone(),
                snapshots: saves,
            })
            .await;
    }

    info!(target:"rgsm::ipc", "Successfully set HEAD to: {:?}", date);
    Ok(())
}

/// Detach a snapshot from its parent, making it a new root node
#[tauri::command]
#[specta::specta]
pub async fn detach_snapshot(
    game: Game,
    date: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Detaching snapshot: {:?} for game: {:?}", date, game);

    let mut saves = game.get_game_snapshots_info().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get game snapshots info: {:?}", e);
        e.to_string()
    })?;

    // Find and update the snapshot
    let snapshot = saves
        .backups
        .iter_mut()
        .find(|s| s.date == date)
        .ok_or_else(|| {
            error!(target:"rgsm::ipc", "Snapshot not found: {:?}", date);
            "Snapshot not found".to_string()
        })?;

    snapshot.parent = None;

    game.set_game_snapshots_info(&saves).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to set game snapshots info: {:?}", e);
        e.to_string()
    })?;

    {
        let pipeline = app_handle.state::<Arc<HookPipeline>>();
        pipeline
            .fire_metadata_changed(&MetadataChangedCtx {
                source: HookSource::UserManual,
                game: game.clone(),
                snapshots: saves,
            })
            .await;
    }

    info!(target:"rgsm::ipc", "Successfully detached snapshot: {:?}", date);
    Ok(())
}

/// Create a new snapshot as a child of a specific parent snapshot
#[tauri::command]
#[specta::specta]
pub async fn create_snapshot_at(
    game: Game,
    describe: String,
    parent_date: Option<String>,
    window: Window,
    app_handle: AppHandle,
) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Creating snapshot at parent: {:?} for game: {:?}", parent_date, game);

    // Temporarily set HEAD to the parent
    let mut saves = game.get_game_snapshots_info().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get game snapshots info: {:?}", e);
        e.to_string()
    })?;

    let original_head = saves.head.clone();
    saves.head = parent_date;

    game.set_game_snapshots_info(&saves).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to set game snapshots info: {:?}", e);
        e.to_string()
    })?;

    // Create the snapshot
    let result = handle_backup_err(game.create_snapshot(&describe).await, window);

    if let Ok(created) = &result {
        let pipeline = app_handle.state::<Arc<HookPipeline>>();
        if let Some(snapshot) = created.snapshots.backups.last().cloned() {
            pipeline
                .fire_snapshot_created(&SnapshotCreatedCtx {
                    source: HookSource::UserManual,
                    game: game.clone(),
                    snapshot,
                    snapshots: created.snapshots.clone(),
                    local_zip_path: created.local_zip_path.clone(),
                    remote_zip_path: created.remote_zip_path.clone(),
                })
                .await;
        }
    }

    // If creation failed, restore original HEAD
    if result.is_err() {
        if let Ok(mut saves) = game.get_game_snapshots_info() {
            saves.head = original_head;
            let _ = game.set_game_snapshots_info(&saves);
        }
    }

    result.map(|_| ())
}

fn handle_backup_err<T>(res: Result<T, BackupError>, window: Window) -> Result<T, String> {
    match res {
        Ok(value) => Ok(value),
        Err(e) => {
            match &e {
                BackupError::Compress(CompressError::Multiple(files)) => {
                    files.iter().for_each(|file| {
                        error!(target:"rgsm::ipc","{}",file);
                        if let BackupFileError::NotExists(path) = file {
                            window
                                .emit(
                                    "Notification",
                                    IpcNotification {
                                        level: NotificationLevel::error,
                                        title: "ERROR".to_string(),
                                        msg: t!(
                                            "backend.backup.backup_file_not_exist",
                                            name = path.to_str().unwrap_or("Cannot get path")
                                        )
                                        .to_string(),
                                    },
                                )
                                .unwrap(); // safe: ipc方法通过前端调用，此时window必然存在
                        }
                    });
                }
                other => {
                    error!(target:"rgsm::ipc","{}",other);
                }
            }
            Err(format!("{}", e))
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_cloud_sync(app_handle: AppHandle) -> Result<CancelCloudSyncResult, String> {
    let manager_state: tauri::State<Arc<CloudSyncTaskManager>> = app_handle.state();
    Ok(Arc::clone(manager_state.inner()).cancel_all().await)
}

/// Fetches the list of importable games from the ludusavi manifest
#[tauri::command]
#[specta::specta]
pub async fn fetch_ludusavi_games(filter_local_only: bool) -> Result<Vec<ImportableGame>, String> {
    info!(target:"rgsm::ipc", "Fetching ludusavi games (filter_local_only: {})", filter_local_only);

    // Get the current managed games
    let config = get_config().map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to get config: {:?}", e);
        e.to_string()
    })?;

    let managed_game_names: Vec<String> = config.games.iter().map(|g| g.name.clone()).collect();

    // Fetch and parse the manifest
    let manifest = ludusavi_manifest::fetch_manifest().await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to fetch manifest: {:?}", e);
        e.to_string()
    })?;

    let games = ludusavi_manifest::parse_manifest_games(
        &manifest,
        &managed_game_names,
        filter_local_only,
        &config,
    );

    info!(target:"rgsm::ipc", "Successfully fetched {} games from ludusavi manifest", games.len());

    Ok(games)
}

/// Gets detailed save paths for a specific game from the ludusavi manifest
#[tauri::command]
#[specta::specta]
pub async fn get_game_save_paths(game_name: String) -> Result<Vec<SavePath>, String> {
    info!(target:"rgsm::ipc", "Getting save paths for game: {}", game_name);

    // Fetch the manifest
    let manifest = ludusavi_manifest::fetch_manifest().await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to fetch manifest: {:?}", e);
        e.to_string()
    })?;

    // Find the game in the manifest
    let game_data = manifest.get(&game_name).ok_or_else(|| {
        warn!(target:"rgsm::ipc", "Game not found in manifest: {}", game_name);
        format!("Game '{}' not found in manifest", game_name)
    })?;

    // Extract save paths
    let paths = ludusavi_manifest::extract_save_paths(&game_name, game_data).map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to extract save paths: {:?}", e);
        e.to_string()
    })?;

    info!(target:"rgsm::ipc", "Found {} save paths for game: {}", paths.len(), game_name);

    Ok(paths)
}

#[tauri::command]
#[specta::specta]
pub fn get_ludusavi_manifest_status() -> Result<LudusaviManifestStatus, String> {
    Ok(ludusavi_manifest::get_manifest_status())
}

#[tauri::command]
#[specta::specta]
pub async fn update_ludusavi_manifest() -> Result<LudusaviManifestStatus, String> {
    ludusavi_manifest::update_manifest_from_remote()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn reset_ludusavi_manifest_to_bundled() -> Result<LudusaviManifestStatus, String> {
    ludusavi_manifest::reset_manifest_to_bundled().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn check_paths(
    paths: Vec<String>,
) -> Result<Vec<path_resolver::PathCheckResult>, String> {
    let config = get_config().map_err(|e| e.to_string())?;
    Ok(path_resolver::check_paths(&paths, &config))
}

/// Gets a list of system font family names
#[tauri::command]
#[specta::specta]
pub fn get_system_fonts() -> Vec<String> {
    info!(target:"rgsm::ipc", "Getting system fonts");
    system_fonts::get_system_fonts()
}

/// Get the local sync state (device-specific, never uploaded).
#[tauri::command]
#[specta::specta]
pub fn get_sync_state() -> Result<cloud_sync::SyncState, String> {
    info!(target:"rgsm::ipc", "Getting sync state");
    cloud_sync::sync_state::load_sync_state().map_err(|e| e.to_string())
}

/// List available config backup files (newest first).
#[tauri::command]
#[specta::specta]
pub fn list_config_backups() -> Vec<String> {
    config::backup::list_config_backups()
        .into_iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect()
}

/// Restore config from a backup by index (0 = most recent).
#[tauri::command]
#[specta::specta]
pub fn restore_config_backup(index: usize) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Restoring config from backup index {}", index);
    config::backup::restore_config_from_backup(index).map_err(|e| e.to_string())
}

/// Sync one game by comparing local and remote snapshots.
#[tauri::command]
#[specta::specta]
pub async fn sync_game(game_name: String) -> Result<SyncGameOutcome, String> {
    info!(target:"rgsm::ipc", "Syncing game: {}", game_name);
    cloud_sync::sync_game_from_config(&game_name)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod test {
    use super::{IpcNotification, NotificationLevel};

    #[test]
    fn test1() {
        let a = serde_json::to_string(&IpcNotification {
            level: NotificationLevel::error,
            title: "title1".to_string(),
            msg: "msg1".to_string(),
        })
        .unwrap(); // safe:测试代码，不应出现错误，可以直接unwrap
        assert_eq!(
            a,
            "{\"level\":\"error\",\"title\":\"title1\",\"msg\":\"msg1\"}"
        )
    }
}
