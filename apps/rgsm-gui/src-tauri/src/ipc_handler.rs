use crate::{quick_actions, sound};
use rgsm_core::backup::{CreatedBy, ExtraBackupItem, Game, GameDraft, GameSnapshots};
use rgsm_core::cloud_sync::{
    self, BatchSyncItemStatus, BatchSyncReport, CancelCloudSyncResult, CloudSyncSessionConfig,
    CloudSyncTaskManager, ConflictResolution, ConflictResolutionOutcome, SyncGameOutcome,
};
use rgsm_core::config::{Config, QuickActionSoundPreferences, get_backup_path, get_config};
use rgsm_core::device::{Device, get_current_device_id};
use rgsm_core::hooks::{HookPipeline, HookSource};
use rgsm_core::ludusavi_manifest::{self, ImportableGame, LudusaviManifestStatus, SavePath};
use rgsm_core::path_launcher::{OpenManagedLocationOutcome, OpenManagedLocationWarning};
use rgsm_core::path_resolver;
use rgsm_core::preclude::*;
use rgsm_core::services::ServiceContext;
use rgsm_core::steam;
use rgsm_core::vn_scanner;
use rgsm_core::{backup, config, system_fonts};

use anyhow::Result;
use log::{debug, error, info, warn};
use rgsm_core::backup::{RestoreNotificationLevel, RestoreNotifier};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, Window};
use tauri_plugin_dialog::DialogExt;
use tauri_specta::Event;

use crate::hooks::HookPipelineState;

/// Adapter: emits restore progress as IpcNotification events via Tauri.
struct TauriRestoreNotifier {
    app: AppHandle,
}

impl RestoreNotifier for TauriRestoreNotifier {
    fn notify(&self, level: RestoreNotificationLevel, title: &str, msg: &str) {
        let notification_level = match level {
            RestoreNotificationLevel::Info => NotificationLevel::info,
            RestoreNotificationLevel::Warning => NotificationLevel::warning,
        };
        if let Err(err) = (IpcNotification {
            level: notification_level,
            title: title.to_string(),
            msg: msg.to_string(),
        })
        .emit(&self.app)
        {
            warn!(target: "rgsm::ipc", "Failed to emit restore notification: {err:?}");
        }
    }
}

/// Helper to create a notifier from an AppHandle
fn notifier(app: &AppHandle) -> TauriRestoreNotifier {
    TauriRestoreNotifier { app: app.clone() }
}

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

fn hook_pipeline<R: tauri::Runtime, M: Manager<R>>(manager: &M) -> Arc<HookPipeline> {
    manager.state::<HookPipelineState>().snapshot()
}

fn svc<R: tauri::Runtime, M: Manager<R>>(manager: &M) -> ServiceContext {
    ServiceContext::new(hook_pipeline(manager))
}

async fn rebuild_pipeline_and_fire_config_saved(
    app_handle: &AppHandle,
    config: Config,
    source: HookSource,
) {
    let pipeline = crate::hooks::rebuild_pipeline(app_handle, &config);
    ServiceContext::new(pipeline)
        .fire_config_saved(config, source)
        .await;
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

/// Tauri Event wrapper for CloudSyncStatus (core type has no Event derive)
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct CloudSyncStatusEvent {
    pub active_jobs: usize,
    pub current_description: Option<String>,
    pub jobs: Vec<cloud_sync::CloudSyncJobInfo>,
}

/// Tauri Event wrapper for CloudSyncError (core type has no Event derive)
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct CloudSyncErrorEvent {
    pub game_name: Option<String>,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct BuildInfo {
    pub version: String,
    pub git_hash: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum OpenPathOutcome {
    Opened,
    Warning { warning: OpenPathWarning },
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Type)]
#[serde(rename_all = "camelCase")]
pub enum OpenPathWarning {
    RegistryOpenUnsupported,
}

impl From<OpenManagedLocationOutcome> for OpenPathOutcome {
    fn from(outcome: OpenManagedLocationOutcome) -> Self {
        match outcome {
            OpenManagedLocationOutcome::Opened => Self::Opened,
            OpenManagedLocationOutcome::Warning(
                OpenManagedLocationWarning::RegistryOpenUnsupported,
            ) => Self::Warning {
                warning: OpenPathWarning::RegistryOpenUnsupported,
            },
        }
    }
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
pub async fn get_build_info() -> BuildInfo {
    BuildInfo {
        version: rgsm_core::version().to_string(),
        git_hash: rgsm_core::git_hash().to_string(),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn open_file_or_folder(path: String) -> Result<OpenPathOutcome, String> {
    info!(target:"rgsm::ipc", "Opening file or folder: {}", path);

    let config = get_config().map_err(|e| e.to_string())?;
    rgsm_core::path_launcher::open_managed_location(&path, None, &config)
        .map(OpenPathOutcome::from)
        .map_err(|e| {
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
    svc(&app_handle)
        .add_game(&game, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to add game: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::ipc", "Successfully added game draft: {:?}", game.name);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn update_game(
    storage_key: String,
    game: GameDraft,
    app_handle: AppHandle,
) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Updating game (storage_key={}): {:?}", storage_key, game);
    svc(&app_handle)
        .update_game(&storage_key, &game, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to update game: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::ipc", "Successfully updated game: {:?}", game.name);
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
    let n = notifier(&app);
    svc(&app)
        .restore_snapshot(&game, &date, HookSource::UserManual, Some(&n))
        .await
        .map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to apply backup: {:?}", e);
            RestoreError::from(e)
        })?;

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
    svc(&app_handle)
        .delete_snapshot(&game, &date, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to delete backup: {:?}", e);
            e.to_string()
        })?;

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
    svc(&app_handle)
        .batch_delete_snapshots(&game, &dates, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to batch delete snapshots: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::ipc", "Successfully batch deleted {} snapshots for game: {:?}", dates.len(), game.name);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_game(game: Game, app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Deleting game: {:?}", game);
    svc(&app_handle)
        .delete_game(&game, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to delete game: {:?}", e);
            e.to_string()
        })?;

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
    use rgsm_core::backup::compute_file_hash;

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
    svc(&app_handle).save_config(&config).await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to set config: {:?}", e);
        e.to_string()
    })?;
    rebuild_pipeline_and_fire_config_saved(&app_handle, config, HookSource::UserManual).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn reset_settings(app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Resetting settings.");
    let config = svc(&app_handle).reset_settings().await.map_err(|e| {
        error!(target:"rgsm::ipc", "Failed to reset settings: {:?}", e);
        e.to_string()
    })?;
    rebuild_pipeline_and_fire_config_saved(&app_handle, config, HookSource::UserManual).await;
    Ok(())
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
    handle_backup_err(
        svc(&app_handle)
            .create_snapshot(&game, &describe, HookSource::UserManual)
            .await,
        window,
    )?;

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
    let n = notifier(&app);
    backup::restore_extra_backup(&game, &date, Some(&n)).map_err(|e| {
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
pub async fn check_cloud_backend(
    session: CloudSyncSessionConfig,
    app_handle: AppHandle,
) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Checking cloud backend: {:?}", session.backend.clone().sanitize());
    match svc(&app_handle).check_cloud_backend(&session).await {
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
    let result = svc(&app_handle)
        .upload_all_from_session(&session, Some(token))
        .await;
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
    let result = svc(&app_handle)
        .download_all_from_session(&session, Some(token))
        .await;

    // After a successful download, the config may contain new games or updated
    // auto-backup settings. Rebuild the hook pipeline so the scheduler syncs.
    if matches!(
        summarize_batch_result(&result).0,
        cloud_sync::CloudSyncJobStatus::Completed
    ) {
        match get_config() {
            Ok(config) => {
                rebuild_pipeline_and_fire_config_saved(&app_handle, config, HookSource::CloudSync)
                    .await;
            }
            Err(err) => {
                warn!(
                    target: "rgsm::ipc",
                    "Failed to reload config after cloud download: {err:?}"
                );
            }
        }
    }

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
    svc(&app_handle)
        .set_snapshot_description(&game, &date, &describe, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to set backup describe: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::ipc", "Successfully set backup {} describe for game: {:?}", date,game);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn backup_all(app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::ipc","Backing up all games.");
    svc(&app_handle)
        .backup_all(HookSource::BatchOperation)
        .await
        .map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to backup all games: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::ipc","Successfully backed up all games.");
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn apply_all(app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::ipc","Applying all backups.");
    let n = notifier(&app_handle);
    svc(&app_handle)
        .apply_all(HookSource::BatchOperation, Some(&n))
        .await
        .map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to apply all backups: {:?}", e);
            e.to_string()
        })?;

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
pub async fn set_game_auto_backup(
    app_handle: AppHandle,
    game_name: String,
    auto_backup: Option<backup::AutoBackupConfig>,
) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Setting auto-backup for '{}': {:?}", game_name, auto_backup);
    svc(&app_handle)
        .set_game_auto_backup(&game_name, auto_backup, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to save config for auto-backup: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::ipc", "Successfully set auto-backup for '{}'", game_name);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_snapshot_created_by(
    app_handle: AppHandle,
    game_name: String,
    snapshot_date: String,
    created_by: CreatedBy,
) -> Result<GameSnapshots, String> {
    info!(
        target:"rgsm::ipc",
        "Setting created_by for '{game_name}' snapshot '{snapshot_date}' to {created_by:?}"
    );
    svc(&app_handle)
        .set_snapshot_created_by(
            &game_name,
            &snapshot_date,
            created_by,
            HookSource::UserManual,
        )
        .await
        .map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to save snapshots after setting created_by: {:?}", e);
            e.to_string()
        })
        .inspect(|_| {
            info!(
                target:"rgsm::ipc",
                "Successfully set created_by for '{game_name}' snapshot '{snapshot_date}'"
            );
        })
}

#[tauri::command]
#[specta::specta]
pub async fn get_auto_backup_status(
    app_handle: AppHandle,
) -> Result<Vec<quick_actions::AutoBackupGameStatus>, String> {
    let scheduler = app_handle.state::<quick_actions::AutoBackupScheduler>();
    Ok(scheduler.get_status().await)
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
    svc(&app_handle)
        .set_snapshot_head(&game, &date, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to set game snapshots info: {:?}", e);
            e.to_string()
        })?;

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
    svc(&app_handle)
        .detach_snapshot(&game, &date, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::ipc", "Failed to detach snapshot: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::ipc", "Successfully detached snapshot: {:?}", date);
    Ok(())
}

/// Create a new snapshot, optionally branching from a specific parent snapshot
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
    handle_backup_err(
        svc(&app_handle)
            .create_snapshot_at(
                &game,
                &describe,
                parent_date,
                CreatedBy::Manual,
                HookSource::UserManual,
            )
            .await,
        window,
    )?;
    Ok(())
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
    store_user_id: Option<String>,
    install_dirs: Option<Vec<String>>,
    steam_id: Option<u32>,
) -> Result<Vec<path_resolver::PathCheckResult>, String> {
    let config = get_config().map_err(|e| e.to_string())?;
    let device = config.devices.get(get_current_device_id());
    let ctx = path_resolver::PathContext {
        install_dirs: install_dirs.unwrap_or_default(),
        steam_id,
        install_dir_cache: None,
        game_roots: device.map(|d| d.game_roots.clone()).unwrap_or_default(),
        store_user_id,
    };
    Ok(path_resolver::check_paths(&paths, Some(&ctx), &config))
}

#[tauri::command]
#[specta::specta]
pub async fn detect_game_roots() -> Result<Vec<String>, String> {
    info!(target:"rgsm::ipc", "Detecting game root directories");
    steam::detect_game_roots().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn detect_store_user_ids() -> Result<Vec<steam::StoreUserIdCandidate>, String> {
    info!(target:"rgsm::ipc", "Detecting Steam user IDs");
    steam::detect_steam_user_ids().map_err(|e| e.to_string())
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

/// Scan given directories for visual novels (e.g. Kirikiri2) and return them as GameDrafts.
#[tauri::command]
#[specta::specta]
pub fn scan_vns(dirs: Vec<String>) -> Result<Vec<GameDraft>, String> {
    info!(target:"rgsm::ipc", "Scanning directories for visual novels: {:?}", dirs);
    Ok(vn_scanner::scan_games(&dirs))
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
pub async fn restore_config_backup(index: usize, app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Restoring config from backup index {}", index);
    let config = svc(&app_handle)
        .restore_config_backup(index)
        .map_err(|e| e.to_string())?;
    rebuild_pipeline_and_fire_config_saved(&app_handle, config, HookSource::UserManual).await;
    Ok(())
}

/// Sync one game by comparing local and remote snapshots.
#[tauri::command]
#[specta::specta]
pub async fn sync_game(
    game_name: String,
    app_handle: AppHandle,
) -> Result<SyncGameOutcome, String> {
    info!(target:"rgsm::ipc", "Syncing game: {}", game_name);
    svc(&app_handle)
        .sync_game(&game_name)
        .await
        .map_err(|e| e.to_string())
}

/// Resolve a user-visible cloud sync conflict for one game.
#[tauri::command]
#[specta::specta]
pub async fn resolve_game_sync_conflict(
    game_name: String,
    resolution: ConflictResolution,
    app_handle: AppHandle,
) -> Result<ConflictResolutionOutcome, String> {
    info!(target:"rgsm::ipc", "Resolving cloud sync conflict for game: {}", game_name);
    svc(&app_handle)
        .resolve_game_conflict(&game_name, resolution)
        .await
        .map_err(|e| e.to_string())
}

/// Retry syncing the shared configuration to the configured cloud backend.
#[tauri::command]
#[specta::specta]
pub async fn sync_config(app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::ipc", "Syncing cloud config");
    svc(&app_handle)
        .sync_config()
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
