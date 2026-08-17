use crate::{quick_actions, sound};
use rgsm_core::backup::{
    CreatedBy, ExtraBackupItem, Game, GameDeviceBinding, GameDraft, GameSnapshots, SaveUnit,
};
use rgsm_core::cloud_sync::v2::{
    CloudArchiveLibraryView, CloudLibraryCutoverReview, CloudLibraryJoinReview, JoinGameDecision,
    MaterializationOutcome, MaterializationPreview,
};
use rgsm_core::cloud_sync::{
    self, BatchSyncItemStatus, BatchSyncReport, CancelCloudSyncResult, CloudBackendCheckReport,
    CloudSyncSessionConfig, CloudSyncTaskManager, ConflictResolution, ConflictResolutionOutcome,
    SyncGameOutcome,
};
use rgsm_core::config::{
    CloudNamespaceGeneration, Config, GameAutomationSettingsDraft, InitialCatchUpPolicy,
    QuickActionSoundPreferences, SyncMode, get_backup_path, get_config,
};
use rgsm_core::device::{Device, get_current_device_id};
use rgsm_core::hooks::{HookPipeline, HookSource};
use rgsm_core::ludusavi_manifest::{self, ImportableGame, LudusaviManifestStatus, SavePath};
use rgsm_core::path_launcher::{OpenManagedLocationOutcome, OpenManagedLocationWarning};
use rgsm_core::path_pattern::{PathPlaceholder, PathPlaceholderDescriptor};
use rgsm_core::path_resolution::ResolutionReport;
use rgsm_core::path_resolver;
use rgsm_core::preclude::*;
use rgsm_core::services::{
    CloudLibraryCutoverOutcome, CloudLibraryJoinOutcome, CloudLibraryStatus,
    CurrentPositionDecision, GameSyncModeOutcome, LiveSaveSyncOptions, ServiceContext,
};
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
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::hooks::HookPipelineState;

pub mod http_commands;

/// Adapter: emits operation progress as HostNotification events through the Host event stream.
struct HostNotifier {
    app: AppHandle,
}

impl RestoreNotifier for HostNotifier {
    fn notify(&self, level: RestoreNotificationLevel, title: &str, msg: &str) {
        let notification_level = match level {
            RestoreNotificationLevel::Info => NotificationLevel::info,
            RestoreNotificationLevel::Warning => NotificationLevel::warning,
        };
        let event = HostNotification {
            level: notification_level,
            title: title.to_string(),
            msg: msg.to_string(),
        };
        crate::http::emit(&self.app, "notification", &event);
    }
}

/// Helper to create a notifier from an AppHandle
fn notifier(app: &AppHandle) -> HostNotifier {
    HostNotifier { app: app.clone() }
}

/// Typed error for restore operations, allowing the frontend to
/// pattern-match on specific failure modes without string parsing.
#[derive(Debug, Serialize, Deserialize, Clone, Type, utoipa::ToSchema, thiserror::Error)]
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
    #[error("A save location must be chosen before restore")]
    RestoreMappingRequired {
        save_unit_id: u32,
        source_dimensions: rgsm_core::path_resolution::CandidateDimensions,
    },
    #[error("A saved restore location is no longer available")]
    StaleRestoreMapping {
        save_unit_id: u32,
        source_dimensions: rgsm_core::path_resolution::CandidateDimensions,
    },
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
            BackupError::RestorePlan(backup::RestorePlanError::MappingRequired {
                save_unit_id,
                source_dimensions,
                ..
            }) => RestoreError::RestoreMappingRequired {
                save_unit_id,
                source_dimensions,
            },
            BackupError::RestorePlan(backup::RestorePlanError::StaleMapping {
                save_unit_id,
                source_dimensions,
                ..
            }) => RestoreError::StaleRestoreMapping {
                save_unit_id,
                source_dimensions,
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

async fn reload_pipeline_and_fire_config_saved(
    app_handle: &AppHandle,
    source: HookSource,
) -> Result<(), String> {
    let config = get_config().map_err(|error| error.to_string())?;
    rebuild_pipeline_and_fire_config_saved(app_handle, config, source).await;
    Ok(())
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
#[derive(Debug, Serialize, Deserialize, Clone, Type, utoipa::ToSchema)]
pub enum NotificationLevel {
    info,
    warning,
    error,
}
#[derive(Debug, Serialize, Deserialize, Clone, Type, utoipa::ToSchema)]
pub struct HostNotification {
    pub level: NotificationLevel,
    pub title: String,
    pub msg: String,
}

/// Transport payload for a cloud synchronization status update
#[derive(Debug, Clone, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct CloudSyncStatusEvent {
    pub active_jobs: usize,
    pub current_description: Option<String>,
    pub jobs: Vec<cloud_sync::CloudSyncJobInfo>,
}

/// Transport payload for a cloud synchronization failure
#[derive(Debug, Clone, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct CloudSyncErrorEvent {
    pub game_name: Option<String>,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct BuildInfo {
    pub version: String,
    pub git_hash: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, utoipa::ToSchema)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum OpenPathOutcome {
    Opened,
    Warning { warning: OpenPathWarning },
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Type, utoipa::ToSchema)]
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

pub async fn open_url(url: String) -> Result<(), String> {
    info!(target:"rgsm::commands", "Opening url: {}", url);
    open::that(url).map_err(|e| {
        error!(target:"rgsm::commands", "Failed to open url: {:?}", e);
        e.to_string()
    })
}

pub async fn get_build_info() -> BuildInfo {
    BuildInfo {
        version: rgsm_core::version().to_string(),
        git_hash: rgsm_core::git_hash().to_string(),
    }
}

pub async fn open_file_or_folder(path: String) -> Result<OpenPathOutcome, String> {
    info!(target:"rgsm::commands", "Opening file or folder: {}", path);

    let config = get_config().map_err(|e| e.to_string())?;
    rgsm_core::path_launcher::open_managed_location(&path, None, &config)
        .map(OpenPathOutcome::from)
        .map_err(|e| {
            error!(target:"rgsm::commands", "Failed to open file or folder: {:?}", e);
            e.to_string()
        })
}

pub async fn get_app_log_dir(app: AppHandle) -> Result<String, String> {
    info!(target:"rgsm::commands", "Getting app log directory");

    let log_dir = app.path().app_log_dir().map_err(|e| {
        error!(target:"rgsm::commands", "Failed to get app log directory: {:?}", e);
        e.to_string()
    })?;

    debug!(target:"rgsm::commands", "Log directory: {}", log_dir.display());
    Ok(log_dir.to_string_lossy().to_string())
}

pub async fn choose_save_file(_app: AppHandle) -> Result<String, String> {
    info!(target:"rgsm::commands", "Opening file dialog.");
    if let Some(path) = rfd::FileDialog::new().pick_file() {
        info!(target:"rgsm::commands","Successfully picked file: {:#?}",path);
        Ok(path.to_string_lossy().into_owned())
    } else {
        warn!(target:"rgsm::commands", "Failed to open dialog or user close the dialog.");
        Err("Failed to open dialog.".to_string())
    }
}

pub async fn choose_save_dir(_app: AppHandle) -> Result<String, String> {
    info!(target:"rgsm::commands","Opening folder dialog.");
    if let Some(path) = rfd::FileDialog::new().pick_folder() {
        info!(target:"rgsm::commands","Successfully picked folder: {:#?}",path);
        Ok(path.to_string_lossy().into_owned())
    } else {
        warn!(target:"rgsm::commands", "Failed to open dialog or user close the dialog.");
        Err("Failed to open dialog.".to_string())
    }
}

pub async fn get_local_config() -> Result<Config, String> {
    info!(target:"rgsm::commands", "Getting local config.");
    get_config().map_err(|e| e.to_string())
}

pub async fn add_game(game: GameDraft, app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::commands", "Adding game draft: {:?}", game);
    svc(&app_handle)
        .add_game(&game, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::commands", "Failed to add game: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::commands", "Successfully added game draft: {:?}", game.name);
    Ok(())
}

pub async fn update_game(
    storage_key: String,
    game: GameDraft,
    app_handle: AppHandle,
) -> Result<(), String> {
    info!(target:"rgsm::commands", "Updating game (storage_key={}): {:?}", storage_key, game);
    svc(&app_handle)
        .update_game(&storage_key, &game, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::commands", "Failed to update game: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::commands", "Successfully updated game: {:?}", game.name);
    Ok(())
}

pub async fn restore_snapshot(
    game: Game,
    date: String,
    app: AppHandle,
) -> Result<(), RestoreError> {
    info!(target:"rgsm::commands", "Applying backup: {:?} for game: {:?}", date, game);
    let n = notifier(&app);
    svc(&app)
        .restore_snapshot(&game, &date, HookSource::UserManual, Some(&n))
        .await
        .map_err(|e| {
            error!(target:"rgsm::commands", "Failed to apply backup: {:?}", e);
            RestoreError::from(e)
        })?;

    info!(target:"rgsm::commands", "Successfully applied backup: {:?} for game: {:?}", date, game);
    Ok(())
}

pub const ACTIVE_CLOUD_LIBRARY_DELETION_REQUIRES_PERMANENT: &str =
    "Use permanent V2 Snapshot deletion for an active Cloud Library";

pub async fn delete_snapshot(
    game: Game,
    date: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    if rgsm_core::config::cloud_namespace_generation().map_err(|error| error.to_string())?
        == CloudNamespaceGeneration::V2
    {
        return Err(ACTIVE_CLOUD_LIBRARY_DELETION_REQUIRES_PERMANENT.into());
    }
    info!(target:"rgsm::commands", "Deleting backup: {:?} for game: {:?}", date, game);
    svc(&app_handle)
        .delete_snapshot(&game, &date, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::commands", "Failed to delete backup: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::commands", "Successfully deleted backup: {:?} for game: {:?}", date, game);
    Ok(())
}

pub async fn batch_delete_snapshots(
    game: Game,
    dates: Vec<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    if rgsm_core::config::cloud_namespace_generation().map_err(|error| error.to_string())?
        == CloudNamespaceGeneration::V2
    {
        return Err(ACTIVE_CLOUD_LIBRARY_DELETION_REQUIRES_PERMANENT.into());
    }
    info!(target:"rgsm::commands", "Batch deleting {} snapshots for game: {:?}", dates.len(), game.name);
    svc(&app_handle)
        .batch_delete_snapshots(&game, &dates, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::commands", "Failed to batch delete snapshots: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::commands", "Successfully batch deleted {} snapshots for game: {:?}", dates.len(), game.name);
    Ok(())
}

pub fn get_cloud_namespace_generation() -> Result<CloudNamespaceGeneration, String> {
    rgsm_core::config::cloud_namespace_generation().map_err(|error| error.to_string())
}

pub async fn delete_game(game: Game, app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::commands", "Deleting game: {:?}", game);
    svc(&app_handle)
        .delete_game(&game, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::commands", "Failed to delete game: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::commands", "Successfully deleted game: {:?}", game);
    Ok(())
}

pub async fn get_game_snapshots_info(game: Game) -> Result<GameSnapshots, String> {
    info!(target:"rgsm::commands", "Getting backup list info for game: {:?}", game);
    match game.get_game_snapshots_info() {
        Ok(snapshots) => Ok(snapshots),
        Err(BackupError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(GameSnapshots::new(game.name))
        }
        Err(error) => {
            error!(target:"rgsm::commands", "Failed to get backup list info: {:?}", error);
            Err(error.to_string())
        }
    }
}

/// Verify archive integrity by comparing the stored hash against a freshly computed one.
/// Returns `true` if the hash matches (or no stored hash exists), `false` if mismatched.
pub async fn verify_archive_integrity(
    archive_path: String,
    expected_hash: Option<String>,
) -> Result<bool, String> {
    use rgsm_core::backup::compute_file_hash;

    let Some(expected) = expected_hash else {
        return Ok(true);
    };
    let actual = compute_file_hash(std::path::Path::new(&archive_path)).map_err(|e| {
        error!(target:"rgsm::commands", "Failed to compute archive hash: {:?}", e);
        e.to_string()
    })?;
    Ok(actual == expected)
}

pub async fn set_config(app_handle: AppHandle, config: Config) -> Result<(), String> {
    debug!(target:"rgsm::commands", "Setting config: {:?}", config.clone().sanitize());
    svc(&app_handle).save_config(&config).await.map_err(|e| {
        error!(target:"rgsm::commands", "Failed to set config: {:?}", e);
        e.to_string()
    })?;
    rebuild_pipeline_and_fire_config_saved(&app_handle, config, HookSource::UserManual).await;
    Ok(())
}

pub async fn reset_settings(app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::commands", "Resetting settings.");
    let config = svc(&app_handle).reset_settings().await.map_err(|e| {
        error!(target:"rgsm::commands", "Failed to reset settings: {:?}", e);
        e.to_string()
    })?;
    rebuild_pipeline_and_fire_config_saved(&app_handle, config, HookSource::UserManual).await;
    Ok(())
}

pub async fn create_snapshot(
    game: Game,
    describe: String,
    window: WebviewWindow,
    app_handle: AppHandle,
) -> Result<(), String> {
    info!(target:"rgsm::commands", "Backing up save for game: {:?}", game);
    let n = notifier(&app_handle);
    handle_backup_err(
        svc(&app_handle)
            .create_snapshot(&game, &describe, HookSource::UserManual, Some(&n))
            .await,
        window,
    )?;

    info!(target:"rgsm::commands", "Successfully backed up save for game: {:?}", game);
    Ok(())
}

pub async fn open_backup_folder(game: Game) -> Result<bool, String> {
    info!(target:"rgsm::commands", "Opening backup folder for game: {:?}", game);
    let backup_path = get_backup_path().map_err(|e| {
        error!(target:"rgsm::commands", "Failed to get backup path: {:?}", e);
        e.to_string()
    })?;
    let p = game.backup_folder_path(&backup_path);
    Ok(open::that(p).is_ok())
}

pub async fn get_game_extra_backups(game: Game) -> Result<Vec<ExtraBackupItem>, String> {
    info!(target:"rgsm::commands", "Getting extra backups for game: {:?}", game);
    backup::list_extra_backups(&game).map_err(|e| {
        error!(target:"rgsm::commands", "Failed to list extra backups: {:?}", e);
        e.to_string()
    })
}

pub async fn delete_extra_backup(game: Game, date: String) -> Result<(), String> {
    info!(target:"rgsm::commands", "Deleting extra backup: {:?} for game: {:?}", date, game);
    backup::delete_extra_backup(&game, &date).map_err(|e| {
        error!(target:"rgsm::commands", "Failed to delete extra backup: {:?}", e);
        e.to_string()
    })
}

pub async fn restore_extra_backup(game: Game, date: String, app: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::commands", "Restoring extra backup: {:?} for game: {:?}", date, game);
    let n = notifier(&app);
    svc(&app)
        .restore_extra_backup(&game, &date, Some(&n))
        .map_err(|e| {
            error!(target:"rgsm::commands", "Failed to restore extra backup: {:?}", e);
            e.to_string()
        })
}

pub async fn open_extra_backup_folder(game: Game) -> Result<bool, String> {
    info!(target:"rgsm::commands", "Opening extra backup folder for game: {:?}", game);
    let p = backup::extra_backup_folder_path(&game).map_err(|e| {
        error!(target:"rgsm::commands", "Failed to get extra backup path: {:?}", e);
        e.to_string()
    })?;
    Ok(open::that(p).is_ok())
}

pub async fn check_cloud_backend(
    session: CloudSyncSessionConfig,
    app_handle: AppHandle,
) -> Result<CloudBackendCheckReport, String> {
    info!(target:"rgsm::commands", "Checking cloud backend: {:?}", session.backend.clone().sanitize());
    match svc(&app_handle).check_cloud_backend(&session).await {
        Ok(report) => {
            if report.is_usable() {
                info!(
                    target:"rgsm::commands",
                    "Checked cloud backend with outcome {:?}: {:?}",
                    report.outcome,
                    session.backend.sanitize()
                );
            } else {
                warn!(
                    target:"rgsm::commands",
                    "Cloud backend check reported unusable backend: {:?}",
                    session.backend.sanitize()
                );
            }
            Ok(report)
        }
        Err(e) => {
            error!(target:"rgsm::commands", "Failed to check cloud backend: {:?}", e);
            Err(e.to_string())
        }
    }
}

pub async fn inspect_cloud_library(app_handle: AppHandle) -> Result<CloudLibraryStatus, String> {
    info!(target:"rgsm::commands", "Inspecting the saved Cloud Library");
    svc(&app_handle)
        .inspect_cloud_library()
        .await
        .map_err(|error| {
            error!(target:"rgsm::commands", "Failed to inspect Cloud Library: {error:?}");
            error.to_string()
        })
}

pub async fn create_cloud_library(
    confirmed: bool,
    app_handle: AppHandle,
) -> Result<CloudLibraryStatus, String> {
    info!(target:"rgsm::commands", "Creating a new Cloud Library");
    crate::cloud_library::create(&app_handle, confirmed)
        .await
        .map_err(|error| {
            error!(target:"rgsm::commands", "Failed to create Cloud Library: {error:?}");
            error.to_string()
        })
}

pub async fn review_cloud_library_join(
    app_handle: AppHandle,
) -> Result<CloudLibraryJoinReview, String> {
    info!(target:"rgsm::commands", "Reviewing an existing Cloud Library");
    crate::cloud_library::review(&app_handle)
        .await
        .map_err(|error| error.to_string())
}

pub async fn join_cloud_library(
    decisions: Vec<JoinGameDecision>,
    confirmed_replacements: bool,
    app_handle: AppHandle,
) -> Result<CloudLibraryJoinOutcome, String> {
    info!(target:"rgsm::commands", "Joining an existing Cloud Library");
    crate::cloud_library::join(&app_handle, &decisions, confirmed_replacements)
        .await
        .map_err(|error| error.to_string())
}

pub async fn review_cloud_library_cutover(
    app_handle: AppHandle,
) -> Result<CloudLibraryCutoverReview, String> {
    info!(target:"rgsm::commands", "Reviewing legacy Cloud Library Cutover");
    crate::cloud_library::review_cutover(&app_handle)
        .await
        .map_err(|error| error.to_string())
}

pub async fn cutover_cloud_library(
    confirmed: bool,
    app_handle: AppHandle,
) -> Result<CloudLibraryCutoverOutcome, String> {
    info!(target:"rgsm::commands", "Cutting over legacy Cloud Library");
    crate::cloud_library::cutover(&app_handle, confirmed)
        .await
        .map_err(|error| error.to_string())
}

pub async fn get_cloud_archive_library(
    app_handle: AppHandle,
) -> Result<CloudArchiveLibraryView, String> {
    svc(&app_handle)
        .cloud_archive_library()
        .await
        .map_err(|error| error.to_string())
}

pub async fn review_v2_game_progress(
    game_id: String,
    app_handle: AppHandle,
) -> Result<rgsm_core::cloud_sync::v2::V2ConflictReview, String> {
    svc(&app_handle)
        .review_v2_game_progress(&game_id)
        .await
        .map_err(|error| error.to_string())
}

pub async fn keep_v2_local_progress(
    game_id: String,
    manifest_revision: u64,
    local_snapshot_id: String,
    app_handle: AppHandle,
) -> Result<rgsm_core::cloud_sync::v2::KeepLocalProgressOutcome, String> {
    let operation_lock = app_handle
        .state::<crate::snapshot_sync::SnapshotSyncRuntimeState>()
        .operation_lock();
    let _guard = operation_lock.lock().await;
    svc(&app_handle)
        .keep_v2_local_progress(&game_id, manifest_revision, &local_snapshot_id)
        .await
        .map_err(|error| error.to_string())
}

pub async fn accept_v2_remote_progress(
    game_id: String,
    manifest_revision: u64,
    expected_local_snapshot_id: Option<String>,
    selected_snapshot_id: String,
    app_handle: AppHandle,
) -> Result<rgsm_core::services::AcceptRemoteProgressOutcome, String> {
    let operation_lock = app_handle
        .state::<crate::snapshot_sync::SnapshotSyncRuntimeState>()
        .operation_lock();
    let _guard = operation_lock.lock().await;
    svc(&app_handle)
        .accept_v2_remote_progress(
            &game_id,
            manifest_revision,
            expected_local_snapshot_id.as_deref(),
            &selected_snapshot_id,
        )
        .await
        .map_err(|error| error.to_string())
}

pub async fn preview_materialize_all(
    app_handle: AppHandle,
) -> Result<MaterializationPreview, String> {
    svc(&app_handle)
        .preview_materialize_all()
        .await
        .map_err(|error| error.to_string())
}

pub async fn upload_cloud_archive(
    game_id: String,
    snapshot_id: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    svc(&app_handle)
        .upload_cloud_archive(&game_id, &snapshot_id)
        .await
        .map_err(|error| error.to_string())
}

pub async fn download_cloud_archive(
    game_id: String,
    snapshot_id: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    svc(&app_handle)
        .download_cloud_archive(&game_id, &snapshot_id)
        .await
        .map_err(|error| error.to_string())
}

pub async fn delete_v2_snapshot(
    game_id: String,
    snapshot_id: String,
    confirmed: bool,
    current_position: Option<CurrentPositionDecision>,
    app_handle: AppHandle,
) -> Result<(), String> {
    ServiceContext::new(app_handle.state::<HookPipelineState>().snapshot())
        .delete_v2_snapshot(&game_id, &snapshot_id, confirmed, current_position)
        .await
        .map_err(|error| error.to_string())
}

pub async fn reset_broken_cloud_library(app_handle: AppHandle) -> Result<(), String> {
    svc(&app_handle)
        .reset_broken_cloud_library()
        .await
        .map_err(|error| error.to_string())?;
    let config = get_config().map_err(|error| error.to_string())?;
    crate::hooks::rebuild_pipeline(&app_handle, &config);
    Ok(())
}

pub async fn set_shared_snapshot_retention(
    game_id: String,
    limit: Option<u32>,
    confirmed: bool,
    app_handle: AppHandle,
) -> Result<rgsm_core::services::SnapshotRetentionOutcome, String> {
    ServiceContext::new(app_handle.state::<HookPipelineState>().snapshot())
        .set_shared_snapshot_retention(&game_id, limit, confirmed)
        .await
        .map_err(|error| error.to_string())
}

pub async fn set_snapshot_retention_protected(
    game_id: String,
    snapshot_id: String,
    retention_protected: bool,
    confirmed: bool,
    app_handle: AppHandle,
) -> Result<rgsm_core::services::SnapshotRetentionOutcome, String> {
    ServiceContext::new(app_handle.state::<HookPipelineState>().snapshot())
        .set_snapshot_retention_protected(&game_id, &snapshot_id, retention_protected, confirmed)
        .await
        .map_err(|error| error.to_string())
}

pub fn get_current_device_game_statuses(
    app_handle: AppHandle,
) -> Result<Vec<rgsm_core::services::DeviceGameStatus>, String> {
    svc(&app_handle)
        .current_device_game_statuses()
        .map_err(|error| error.to_string())
}

pub async fn set_device_game_visibility(
    game_id: String,
    visible: bool,
    app_handle: AppHandle,
) -> Result<rgsm_core::services::DeviceGameStatus, String> {
    let status = svc(&app_handle)
        .set_device_game_visibility(&game_id, visible)
        .await
        .map_err(|error| error.to_string())?;
    reload_pipeline_and_fire_config_saved(&app_handle, HookSource::UserManual).await?;
    Ok(status)
}

pub async fn set_device_game_managed(
    game_id: String,
    managed: bool,
    confirmed: bool,
    app_handle: AppHandle,
) -> Result<rgsm_core::services::DeviceGameStatus, String> {
    let status = svc(&app_handle)
        .set_device_game_managed(&game_id, managed, confirmed)
        .await
        .map_err(|error| error.to_string())?;
    reload_pipeline_and_fire_config_saved(&app_handle, HookSource::UserManual).await?;
    Ok(status)
}

pub async fn evict_local_archive(
    game_id: String,
    snapshot_id: String,
    confirmed: bool,
    app_handle: AppHandle,
) -> Result<bool, String> {
    svc(&app_handle)
        .evict_local_archive(&game_id, &snapshot_id, confirmed)
        .await
        .map_err(|error| error.to_string())
}

pub async fn evict_cloud_archive(
    game_id: String,
    snapshot_id: String,
    confirmed: bool,
    app_handle: AppHandle,
) -> Result<bool, String> {
    svc(&app_handle)
        .evict_cloud_archive(&game_id, &snapshot_id, confirmed)
        .await
        .map_err(|error| error.to_string())
}

pub async fn get_cloud_device_profiles(
    app_handle: AppHandle,
) -> Result<Vec<rgsm_core::services::CloudDeviceProfileView>, String> {
    svc(&app_handle)
        .cloud_device_profiles()
        .await
        .map_err(|error| error.to_string())
}

pub async fn remove_cloud_device_profile(
    device_id: String,
    confirmed: bool,
    app_handle: AppHandle,
) -> Result<rgsm_core::cloud_sync::v2::DeviceProfileRemovalOutcome, String> {
    svc(&app_handle)
        .remove_cloud_device_profile(&device_id, confirmed)
        .await
        .map_err(|error| error.to_string())
}

pub async fn get_deleted_cloud_games(
    app_handle: AppHandle,
) -> Result<Vec<rgsm_core::services::DeletedCloudGameView>, String> {
    svc(&app_handle)
        .deleted_cloud_games()
        .await
        .map_err(|error| error.to_string())
}

pub async fn permanently_delete_cloud_game(
    game_id: String,
    confirmed: bool,
    app_handle: AppHandle,
) -> Result<rgsm_core::cloud_sync::v2::SharedGameDeletionOutcome, String> {
    svc(&app_handle)
        .permanently_delete_cloud_game(&game_id, confirmed)
        .await
        .map_err(|error| error.to_string())
}

pub async fn materialize_all_cloud_archives(
    app_handle: AppHandle,
) -> Result<MaterializationOutcome, String> {
    let manager = Arc::clone(app_handle.state::<Arc<CloudSyncTaskManager>>().inner());
    let (job_id, token) = manager
        .begin_manual_job("Downloading all Cloud Snapshots".into())
        .await;
    let result = svc(&app_handle)
        .materialize_all_cloud_archives(&token)
        .await;
    let status = if result.is_ok() {
        cloud_sync::CloudSyncJobStatus::Completed
    } else if token.is_cancelled() {
        cloud_sync::CloudSyncJobStatus::Cancelled
    } else {
        cloud_sync::CloudSyncJobStatus::Failed
    };
    let error = result.as_ref().err().map(ToString::to_string);
    manager
        .finish_manual_job(job_id, "Downloading all Cloud Snapshots", status, error)
        .await;
    result.map_err(|error| error.to_string())
}

pub async fn set_game_sync_mode(
    game_id: String,
    enabled: bool,
    mode: SyncMode,
    initial_catch_up: InitialCatchUpPolicy,
    live_save: Option<LiveSaveSyncOptions>,
    app_handle: AppHandle,
) -> Result<GameSyncModeOutcome, String> {
    crate::cloud_library::set_game_sync_mode(
        &app_handle,
        &game_id,
        enabled,
        mode,
        initial_catch_up,
        live_save,
    )
    .await
    .map_err(|error| error.to_string())
}

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
                    target: "rgsm::commands",
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

pub async fn set_snapshot_description(
    game: Game,
    date: String,
    describe: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    info!(target:"rgsm::commands", "Setting backup describe for game: {:?}", game);
    svc(&app_handle)
        .set_snapshot_description(&game, &date, &describe, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::commands", "Failed to set backup describe: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::commands", "Successfully set backup {} describe for game: {:?}", date,game);
    Ok(())
}

pub async fn backup_all(app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::commands","Backing up all games.");
    svc(&app_handle)
        .backup_all(HookSource::BatchOperation)
        .await
        .map_err(|e| {
            error!(target:"rgsm::commands", "Failed to backup all games: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::commands","Successfully backed up all games.");
    Ok(())
}

pub async fn apply_all(app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::commands","Applying all backups.");
    let n = notifier(&app_handle);
    svc(&app_handle)
        .apply_all(HookSource::BatchOperation, Some(&n))
        .await
        .map_err(|e| {
            error!(target:"rgsm::commands", "Failed to apply all backups: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::commands","Successfully applied all backups.");
    Ok(())
}

pub async fn set_quick_backup_game(app_handle: AppHandle, game: Game) -> Result<(), String> {
    info!(target:"rgsm::commands","Setting quick backup game to: {:?}", game);
    let manager_state: tauri::State<Arc<quick_actions::QuickActionManager>> = app_handle.state();
    let manager = Arc::clone(manager_state.inner());
    manager
        .set_quick_backup_game(game.clone())
        .await
        .map_err(|e| {
            error!(target:"rgsm::commands", "Failed to set quick backup game: {:?}", e);
            e.to_string()
        })?;
    info!(target:"rgsm::commands","Successfully set quick backup game to: {:?}", game);
    Ok(())
}

pub async fn set_game_auto_backup(
    app_handle: AppHandle,
    game_name: String,
    auto_backup: Option<backup::AutoBackupConfig>,
) -> Result<(), String> {
    info!(target:"rgsm::commands", "Setting auto-backup for '{}': {:?}", game_name, auto_backup);
    svc(&app_handle)
        .set_game_auto_backup(&game_name, auto_backup, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::commands", "Failed to save config for auto-backup: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::commands", "Successfully set auto-backup for '{}'", game_name);
    Ok(())
}

pub async fn set_game_automation(
    app_handle: AppHandle,
    storage_key: String,
    automation: Option<GameAutomationSettingsDraft>,
) -> Result<(), String> {
    info!(
        target:"rgsm::commands",
        "Setting game automation for '{}': {:?}",
        storage_key,
        automation
    );
    if let Some(automation) = &automation {
        quick_actions::validate_game_automation_target(&storage_key, automation).map_err(|e| {
            error!(target:"rgsm::commands", "Invalid game automation target: {:?}", e);
            e.to_string()
        })?;
    }
    svc(&app_handle)
        .set_game_automation(&storage_key, automation, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::commands", "Failed to save game automation: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::commands", "Successfully set game automation for '{}'", storage_key);
    Ok(())
}

pub async fn set_game_auto_save_settings(
    app_handle: AppHandle,
    storage_key: String,
    auto_backup: Option<backup::AutoBackupConfig>,
    automation: Option<GameAutomationSettingsDraft>,
) -> Result<(), String> {
    info!(
        target:"rgsm::commands",
        "Setting auto-save settings for '{}': auto_backup={:?}, automation={:?}",
        storage_key,
        auto_backup,
        automation
    );
    if let Some(automation) = &automation {
        quick_actions::validate_game_automation_target(&storage_key, automation).map_err(|e| {
            error!(target:"rgsm::commands", "Invalid game automation target: {:?}", e);
            e.to_string()
        })?;
    }

    svc(&app_handle)
        .set_game_auto_save_settings(
            &storage_key,
            auto_backup,
            automation,
            HookSource::UserManual,
        )
        .await
        .map_err(|e| {
            error!(target:"rgsm::commands", "Failed to save auto-save settings: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::commands", "Successfully set auto-save settings for '{}'", storage_key);
    Ok(())
}

pub async fn set_snapshot_created_by(
    app_handle: AppHandle,
    game_name: String,
    snapshot_date: String,
    created_by: CreatedBy,
) -> Result<GameSnapshots, String> {
    if rgsm_core::config::cloud_namespace_generation().map_err(|error| error.to_string())?
        == CloudNamespaceGeneration::V2
    {
        return Err(
            "Use V2 retention protection without changing Snapshot creation provenance".into(),
        );
    }
    info!(
        target:"rgsm::commands",
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
            error!(target:"rgsm::commands", "Failed to save snapshots after setting created_by: {:?}", e);
            e.to_string()
        })
        .inspect(|_| {
            info!(
                target:"rgsm::commands",
                "Successfully set created_by for '{game_name}' snapshot '{snapshot_date}'"
            );
        })
}

pub async fn get_auto_backup_status(
    app_handle: AppHandle,
) -> Result<Vec<quick_actions::AutoBackupGameStatus>, String> {
    let scheduler = app_handle.state::<quick_actions::AutoBackupScheduler>();
    Ok(scheduler.get_status().await)
}

pub fn list_running_processes() -> Result<Vec<crate::process_util::RunningProcessOption>, String> {
    crate::process_util::list_running_processes().map_err(|e| e.to_string())
}

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

pub async fn stop_sound_playback(app: AppHandle) -> Result<(), String> {
    let manager = app.state::<sound::SoundManager>();
    manager.stop().await.map_err(|err| {
        error!(target: "rgsm::sound", "Failed to stop sound: {err:?}");
        err.to_string()
    })
}

pub async fn choose_quick_action_sound_file(app: AppHandle) -> Result<String, String> {
    sound::choose_quick_action_sound_file(&app)
}

/// Resolves a path string containing variables to an actual filesystem path
///
/// This command allows the frontend to resolve paths with variables like <home>, <winAppData>, etc.
pub async fn resolve_path(path: String) -> Result<String, String> {
    info!(target:"rgsm::commands", "Resolving path: {}", path);

    let config = get_config().map_err(|e| {
        error!(target:"rgsm::commands", "Failed to get config: {:?}", e);
        e.to_string()
    })?;

    let resolved_path = path_resolver::resolve_path(&path, None, &config).map_err(|e| {
        error!(target:"rgsm::commands", "Failed to resolve path: {:?}", e);
        e.to_string()
    })?;

    let path_str = resolved_path.to_str().ok_or_else(|| {
        let err = "Failed to convert resolved path to string";
        error!(target:"rgsm::commands", "{}", err);
        err.to_string()
    })?;

    info!(target:"rgsm::commands", "Successfully resolved path: {} -> {}", path, path_str);
    Ok(path_str.to_string())
}

/// Returns the current device, if not found, returns a default device
pub async fn get_current_device_info() -> Result<Device, String> {
    info!(target:"rgsm::commands", "Getting current device info");

    let device_id = get_current_device_id();
    let config = get_config().map_err(|e| {
        error!(target:"rgsm::commands", "Failed to get config: {:?}", e);
        e.to_string()
    })?;

    Ok(config.devices.get(device_id).cloned().unwrap_or_default())
}

/// Set the HEAD pointer to a specific snapshot
/// This changes which snapshot new snapshots will branch from
pub async fn set_snapshot_head(
    game: Game,
    date: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    info!(target:"rgsm::commands", "Setting HEAD to snapshot: {:?} for game: {:?}", date, game);
    svc(&app_handle)
        .set_snapshot_head(&game, &date, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::commands", "Failed to set game snapshots info: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::commands", "Successfully set HEAD to: {:?}", date);
    Ok(())
}

/// Detach a snapshot from its parent, making it a new root node
pub async fn detach_snapshot(
    game: Game,
    date: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    info!(target:"rgsm::commands", "Detaching snapshot: {:?} for game: {:?}", date, game);
    svc(&app_handle)
        .detach_snapshot(&game, &date, HookSource::UserManual)
        .await
        .map_err(|e| {
            error!(target:"rgsm::commands", "Failed to detach snapshot: {:?}", e);
            e.to_string()
        })?;

    info!(target:"rgsm::commands", "Successfully detached snapshot: {:?}", date);
    Ok(())
}

/// Create a new snapshot, optionally branching from a specific parent snapshot
pub async fn create_snapshot_at(
    game: Game,
    describe: String,
    parent_date: Option<String>,
    window: WebviewWindow,
    app_handle: AppHandle,
) -> Result<(), String> {
    info!(target:"rgsm::commands", "Creating snapshot at parent: {:?} for game: {:?}", parent_date, game);
    let n = notifier(&app_handle);
    handle_backup_err(
        svc(&app_handle)
            .create_snapshot_at(
                &game,
                &describe,
                parent_date,
                CreatedBy::Manual,
                HookSource::UserManual,
                Some(&n),
            )
            .await,
        window,
    )?;
    Ok(())
}

fn handle_backup_err<T>(res: Result<T, BackupError>, window: WebviewWindow) -> Result<T, String> {
    match res {
        Ok(value) => Ok(value),
        Err(e) => {
            match &e {
                BackupError::Compress(CompressError::Multiple(files)) => {
                    files.iter().for_each(|file| {
                        error!(target:"rgsm::commands","{}",file);
                        if let BackupFileError::NotExists(path) = file {
                            crate::http::emit(
                                window.app_handle(),
                                "notification",
                                &HostNotification {
                                    level: NotificationLevel::error,
                                    title: "ERROR".to_string(),
                                    msg: t!(
                                        "backend.backup.backup_file_not_exist",
                                        name = path.to_str().unwrap_or("Cannot get path")
                                    )
                                    .to_string(),
                                },
                            );
                        }
                    });
                }
                other => {
                    error!(target:"rgsm::commands","{}",other);
                }
            }
            Err(format!("{}", e))
        }
    }
}

pub async fn cancel_cloud_sync(app_handle: AppHandle) -> Result<CancelCloudSyncResult, String> {
    let manager_state: tauri::State<Arc<CloudSyncTaskManager>> = app_handle.state();
    Ok(Arc::clone(manager_state.inner()).cancel_all().await)
}

/// Fetches the list of importable games from the ludusavi manifest
pub async fn fetch_ludusavi_games(filter_local_only: bool) -> Result<Vec<ImportableGame>, String> {
    info!(target:"rgsm::commands", "Fetching ludusavi games (filter_local_only: {})", filter_local_only);

    // Get the current managed games
    let config = get_config().map_err(|e| {
        error!(target:"rgsm::commands", "Failed to get config: {:?}", e);
        e.to_string()
    })?;

    let managed_game_names: Vec<String> = config.games.iter().map(|g| g.name.clone()).collect();

    // Fetch and parse the manifest
    let manifest = ludusavi_manifest::fetch_manifest().await.map_err(|e| {
        error!(target:"rgsm::commands", "Failed to fetch manifest: {:?}", e);
        e.to_string()
    })?;

    let games = ludusavi_manifest::parse_manifest_games(
        &manifest,
        &managed_game_names,
        filter_local_only,
        &config,
    );

    info!(target:"rgsm::commands", "Successfully fetched {} games from ludusavi manifest", games.len());

    Ok(games)
}

/// Gets detailed save paths for a specific game from the ludusavi manifest
pub async fn get_game_save_paths(game_name: String) -> Result<Vec<SavePath>, String> {
    info!(target:"rgsm::commands", "Getting save paths for game: {}", game_name);

    // Fetch the manifest
    let manifest = ludusavi_manifest::fetch_manifest().await.map_err(|e| {
        error!(target:"rgsm::commands", "Failed to fetch manifest: {:?}", e);
        e.to_string()
    })?;

    // Find the game in the manifest
    let game_data = manifest.get(&game_name).ok_or_else(|| {
        warn!(target:"rgsm::commands", "Game not found in manifest: {}", game_name);
        format!("Game '{}' not found in manifest", game_name)
    })?;

    // Extract save paths
    let paths = ludusavi_manifest::extract_save_paths(&game_name, game_data).map_err(|e| {
        error!(target:"rgsm::commands", "Failed to extract save paths: {:?}", e);
        e.to_string()
    })?;

    info!(target:"rgsm::commands", "Found {} save paths for game: {}", paths.len(), game_name);

    Ok(paths)
}

pub fn get_path_placeholder_catalog() -> Vec<PathPlaceholderDescriptor> {
    PathPlaceholder::catalog()
}

pub fn preview_save_unit_resolution(
    game: Game,
    save_unit: SaveUnit,
    app_handle: AppHandle,
) -> Result<ResolutionReport, String> {
    let config = get_config().map_err(|error| error.to_string())?;
    Ok(svc(&app_handle).resolve_save_unit(&config, &game, &save_unit))
}

pub async fn set_game_device_binding(
    identity: String,
    binding: GameDeviceBinding,
    app_handle: AppHandle,
) -> Result<(), String> {
    svc(&app_handle)
        .set_game_device_binding(&identity, binding, HookSource::UserManual)
        .await
        .map_err(|error| error.to_string())
}

pub async fn save_restore_mapping(
    identity: String,
    save_unit_id: u32,
    source_dimensions: rgsm_core::path_resolution::CandidateDimensions,
    target_candidate_ids: Vec<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    svc(&app_handle)
        .save_restore_mapping(
            &identity,
            save_unit_id,
            source_dimensions,
            target_candidate_ids,
            HookSource::UserManual,
        )
        .await
        .map_err(|error| error.to_string())
}

pub fn get_ludusavi_manifest_status() -> Result<LudusaviManifestStatus, String> {
    Ok(ludusavi_manifest::get_manifest_status())
}

pub async fn update_ludusavi_manifest() -> Result<LudusaviManifestStatus, String> {
    ludusavi_manifest::update_manifest_from_remote()
        .await
        .map_err(|e| e.to_string())
}

pub fn reset_ludusavi_manifest_to_bundled() -> Result<LudusaviManifestStatus, String> {
    ludusavi_manifest::reset_manifest_to_bundled().map_err(|e| e.to_string())
}

pub async fn check_paths(
    paths: Vec<String>,
    store_user_id: Option<String>,
    install_dirs: Option<Vec<String>>,
    steam_id: Option<u32>,
    app_handle: AppHandle,
) -> Result<Vec<path_resolver::PathCheckResult>, String> {
    let config = get_config().map_err(|e| e.to_string())?;
    let install_dirs = install_dirs.unwrap_or_default();
    Ok(svc(&app_handle).check_ad_hoc_paths(
        &config,
        &paths,
        store_user_id.as_deref(),
        &install_dirs,
        steam_id,
    ))
}

pub async fn detect_game_roots() -> Result<Vec<String>, String> {
    info!(target:"rgsm::commands", "Detecting game root directories");
    steam::detect_game_roots().map_err(|e| e.to_string())
}

pub async fn detect_store_user_ids() -> Result<Vec<steam::StoreUserIdCandidate>, String> {
    info!(target:"rgsm::commands", "Detecting Steam user IDs");
    steam::detect_steam_user_ids().map_err(|e| e.to_string())
}

/// Gets a list of system font family names
pub fn get_system_fonts() -> Vec<String> {
    info!(target:"rgsm::commands", "Getting system fonts");
    system_fonts::get_system_fonts()
}

/// Get the local sync state (device-specific, never uploaded).
pub fn get_sync_state() -> Result<cloud_sync::SyncState, String> {
    info!(target:"rgsm::commands", "Getting sync state");
    cloud_sync::sync_state::load_sync_state().map_err(|e| e.to_string())
}

/// Scan given directories for visual novels (e.g. Kirikiri2) and return them as GameDrafts.
pub fn scan_vns(dirs: Vec<String>) -> Result<Vec<GameDraft>, String> {
    info!(target:"rgsm::commands", "Scanning directories for visual novels: {:?}", dirs);
    Ok(vn_scanner::scan_games(&dirs))
}

/// List available config backup files (newest first).
pub fn list_config_backups() -> Vec<String> {
    config::backup::list_config_backups()
        .into_iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect()
}

/// Restore config from a backup by index (0 = most recent).
pub async fn restore_config_backup(index: usize, app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::commands", "Restoring config from backup index {}", index);
    let config = svc(&app_handle)
        .restore_config_backup(index)
        .map_err(|e| e.to_string())?;
    rebuild_pipeline_and_fire_config_saved(&app_handle, config, HookSource::UserManual).await;
    Ok(())
}

/// Sync one game by comparing local and remote snapshots.
pub async fn sync_game(
    game_name: String,
    app_handle: AppHandle,
) -> Result<SyncGameOutcome, String> {
    info!(target:"rgsm::commands", "Syncing game: {}", game_name);
    svc(&app_handle)
        .sync_game(&game_name)
        .await
        .map_err(|e| e.to_string())
}

/// Resolve a user-visible cloud sync conflict for one game.
pub async fn resolve_game_sync_conflict(
    game_name: String,
    resolution: ConflictResolution,
    app_handle: AppHandle,
) -> Result<ConflictResolutionOutcome, String> {
    info!(target:"rgsm::commands", "Resolving cloud sync conflict for game: {}", game_name);
    svc(&app_handle)
        .resolve_game_conflict(&game_name, resolution)
        .await
        .map_err(|e| e.to_string())
}

/// Retry syncing the shared configuration to the configured cloud backend.
pub async fn sync_config(app_handle: AppHandle) -> Result<(), String> {
    info!(target:"rgsm::commands", "Syncing cloud config");
    svc(&app_handle)
        .sync_config()
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod test {
    use super::{HostNotification, NotificationLevel};

    #[test]
    fn test1() {
        let a = serde_json::to_string(&HostNotification {
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
