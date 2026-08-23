use super::*;
use crate::commands;

use axum::{Json, Router, extract::State, routing::post};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use crate::http::{ApiError, HttpHostState};

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenUrlRequest {
    pub url: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/open-url",
    operation_id = "openUrl",
    request_body = OpenUrlRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 409, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_open_url(Json(request): Json<OpenUrlRequest>) -> Result<Json<()>, ApiError> {
    commands::open_url(request.url)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/get-build-info",
    operation_id = "getBuildInfo",
    responses((status = 200, body = BuildInfo), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_get_build_info() -> Result<Json<BuildInfo>, ApiError> {
    Ok(Json(commands::get_build_info().await))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenFileOrFolderRequest {
    pub path: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/open-file-or-folder",
    operation_id = "openFileOrFolder",
    request_body = OpenFileOrFolderRequest,
    responses((status = 200, body = OpenPathOutcome), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_open_file_or_folder(
    Json(request): Json<OpenFileOrFolderRequest>,
) -> Result<Json<OpenPathOutcome>, ApiError> {
    commands::open_file_or_folder(request.path)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/get-app-log-dir",
    operation_id = "getAppLogDir",
    responses((status = 200, body = String), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_get_app_log_dir(
    State(state): State<HttpHostState>,
) -> Result<Json<String>, ApiError> {
    commands::get_app_log_dir(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/choose-save-file",
    operation_id = "chooseSaveFile",
    responses((status = 200, body = String), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_choose_save_file(
    State(state): State<HttpHostState>,
) -> Result<Json<String>, ApiError> {
    commands::choose_save_file(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/choose-save-dir",
    operation_id = "chooseSaveDir",
    responses((status = 200, body = String), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_choose_save_dir(
    State(state): State<HttpHostState>,
) -> Result<Json<String>, ApiError> {
    commands::choose_save_dir(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/get-local-config",
    operation_id = "getLocalConfig",
    responses((status = 200, body = Config), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_get_local_config() -> Result<Json<Config>, ApiError> {
    commands::get_local_config()
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddGameRequest {
    pub game: GameDraft,
}

#[utoipa::path(
    post,
    path = "/api/v1/add-game",
    operation_id = "addGame",
    request_body = AddGameRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 409, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_add_game(
    State(state): State<HttpHostState>,
    Json(request): Json<AddGameRequest>,
) -> Result<Json<()>, ApiError> {
    commands::add_game(request.game, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGameRequest {
    pub storage_key: String,
    pub game: GameDraft,
}

#[utoipa::path(
    post,
    path = "/api/v1/update-game",
    operation_id = "updateGame",
    request_body = UpdateGameRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_update_game(
    State(state): State<HttpHostState>,
    Json(request): Json<UpdateGameRequest>,
) -> Result<Json<()>, ApiError> {
    commands::update_game(request.storage_key, request.game, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSnapshotRequest {
    pub game: Game,
    pub date: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/restore-snapshot",
    operation_id = "restoreSnapshot",
    request_body = RestoreSnapshotRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_restore_snapshot(
    State(state): State<HttpHostState>,
    Json(request): Json<RestoreSnapshotRequest>,
) -> Result<Json<()>, ApiError> {
    commands::restore_snapshot(request.game, request.date, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_serializable)
}

fn map_snapshot_deletion_error(error: String) -> ApiError {
    if error == commands::ACTIVE_CLOUD_LIBRARY_DELETION_REQUIRES_PERMANENT {
        ApiError::conflict(error)
    } else {
        ApiError::from_command(error)
    }
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSnapshotRequest {
    pub game: Game,
    pub date: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/delete-snapshot",
    operation_id = "deleteSnapshot",
    request_body = DeleteSnapshotRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 409, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_delete_snapshot(
    State(state): State<HttpHostState>,
    Json(request): Json<DeleteSnapshotRequest>,
) -> Result<Json<()>, ApiError> {
    commands::delete_snapshot(request.game, request.date, state.app().clone())
        .await
        .map(Json)
        .map_err(map_snapshot_deletion_error)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchDeleteSnapshotsRequest {
    pub game: Game,
    pub dates: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/batch-delete-snapshots",
    operation_id = "batchDeleteSnapshots",
    request_body = BatchDeleteSnapshotsRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 409, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_batch_delete_snapshots(
    State(state): State<HttpHostState>,
    Json(request): Json<BatchDeleteSnapshotsRequest>,
) -> Result<Json<()>, ApiError> {
    commands::batch_delete_snapshots(request.game, request.dates, state.app().clone())
        .await
        .map(Json)
        .map_err(map_snapshot_deletion_error)
}

#[utoipa::path(
    post,
    path = "/api/v1/get-cloud-namespace-generation",
    operation_id = "getCloudNamespaceGeneration",
    responses((status = 200, body = CloudNamespaceGeneration), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_get_cloud_namespace_generation()
-> Result<Json<CloudNamespaceGeneration>, ApiError> {
    commands::get_cloud_namespace_generation()
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteGameRequest {
    pub game: Game,
}

#[utoipa::path(
    post,
    path = "/api/v1/delete-game",
    operation_id = "deleteGame",
    request_body = DeleteGameRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_delete_game(
    State(state): State<HttpHostState>,
    Json(request): Json<DeleteGameRequest>,
) -> Result<Json<()>, ApiError> {
    commands::delete_game(request.game, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetGameSnapshotsInfoRequest {
    pub game: Game,
}

#[utoipa::path(
    post,
    path = "/api/v1/get-game-snapshots-info",
    operation_id = "getGameSnapshotsInfo",
    request_body = GetGameSnapshotsInfoRequest,
    responses((status = 200, body = GameSnapshots), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_get_game_snapshots_info(
    Json(request): Json<GetGameSnapshotsInfoRequest>,
) -> Result<Json<GameSnapshots>, ApiError> {
    commands::get_game_snapshots_info(request.game)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifyArchiveIntegrityRequest {
    pub archive_path: String,
    pub expected_hash: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/verify-archive-integrity",
    operation_id = "verifyArchiveIntegrity",
    request_body = VerifyArchiveIntegrityRequest,
    responses((status = 200, body = bool), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_verify_archive_integrity(
    Json(request): Json<VerifyArchiveIntegrityRequest>,
) -> Result<Json<bool>, ApiError> {
    commands::verify_archive_integrity(request.archive_path, request.expected_hash)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetConfigRequest {
    pub config: Config,
}

#[utoipa::path(
    post,
    path = "/api/v1/set-config",
    operation_id = "setConfig",
    request_body = SetConfigRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_set_config(
    State(state): State<HttpHostState>,
    Json(request): Json<SetConfigRequest>,
) -> Result<Json<()>, ApiError> {
    commands::set_config(state.app().clone(), request.config)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/reset-settings",
    operation_id = "resetSettings",
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_reset_settings(State(state): State<HttpHostState>) -> Result<Json<()>, ApiError> {
    commands::reset_settings(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSnapshotRequest {
    pub game: Game,
    pub describe: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/create-snapshot",
    operation_id = "createSnapshot",
    request_body = CreateSnapshotRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_create_snapshot(
    State(state): State<HttpHostState>,
    Json(request): Json<CreateSnapshotRequest>,
) -> Result<Json<()>, ApiError> {
    let window = state
        .app()
        .get_webview_window("main")
        .ok_or_else(|| ApiError::unavailable("Main window is not available"))?;
    commands::create_snapshot(request.game, request.describe, window, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenBackupFolderRequest {
    pub game: Game,
}

#[utoipa::path(
    post,
    path = "/api/v1/open-backup-folder",
    operation_id = "openBackupFolder",
    request_body = OpenBackupFolderRequest,
    responses((status = 200, body = bool), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_open_backup_folder(
    Json(request): Json<OpenBackupFolderRequest>,
) -> Result<Json<bool>, ApiError> {
    commands::open_backup_folder(request.game)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetGameExtraBackupsRequest {
    pub game: Game,
}

#[utoipa::path(
    post,
    path = "/api/v1/get-game-extra-backups",
    operation_id = "getGameExtraBackups",
    request_body = GetGameExtraBackupsRequest,
    responses((status = 200, body = Vec<ExtraBackupItem>), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_get_game_extra_backups(
    Json(request): Json<GetGameExtraBackupsRequest>,
) -> Result<Json<Vec<ExtraBackupItem>>, ApiError> {
    commands::get_game_extra_backups(request.game)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteExtraBackupRequest {
    pub game: Game,
    pub date: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/delete-extra-backup",
    operation_id = "deleteExtraBackup",
    request_body = DeleteExtraBackupRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_delete_extra_backup(
    Json(request): Json<DeleteExtraBackupRequest>,
) -> Result<Json<()>, ApiError> {
    commands::delete_extra_backup(request.game, request.date)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestoreExtraBackupRequest {
    pub game: Game,
    pub date: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/restore-extra-backup",
    operation_id = "restoreExtraBackup",
    request_body = RestoreExtraBackupRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_restore_extra_backup(
    State(state): State<HttpHostState>,
    Json(request): Json<RestoreExtraBackupRequest>,
) -> Result<Json<()>, ApiError> {
    commands::restore_extra_backup(request.game, request.date, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenExtraBackupFolderRequest {
    pub game: Game,
}

#[utoipa::path(
    post,
    path = "/api/v1/open-extra-backup-folder",
    operation_id = "openExtraBackupFolder",
    request_body = OpenExtraBackupFolderRequest,
    responses((status = 200, body = bool), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_open_extra_backup_folder(
    Json(request): Json<OpenExtraBackupFolderRequest>,
) -> Result<Json<bool>, ApiError> {
    commands::open_extra_backup_folder(request.game)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckCloudBackendRequest {
    pub session: CloudSyncSessionConfig,
}

#[utoipa::path(
    post,
    path = "/api/v1/check-cloud-backend",
    operation_id = "checkCloudBackend",
    request_body = CheckCloudBackendRequest,
    responses((status = 200, body = CloudBackendCheckReport), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_check_cloud_backend(
    State(state): State<HttpHostState>,
    Json(request): Json<CheckCloudBackendRequest>,
) -> Result<Json<CloudBackendCheckReport>, ApiError> {
    commands::check_cloud_backend(request.session, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/inspect-cloud-library",
    operation_id = "inspectCloudLibrary",
    responses((status = 200, body = CloudLibraryStatus), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_inspect_cloud_library(
    State(state): State<HttpHostState>,
) -> Result<Json<CloudLibraryStatus>, ApiError> {
    commands::inspect_cloud_library(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateCloudLibraryRequest {
    pub confirmed: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/create-cloud-library",
    operation_id = "createCloudLibrary",
    request_body = CreateCloudLibraryRequest,
    responses((status = 200, body = CloudLibraryStatus), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_create_cloud_library(
    State(state): State<HttpHostState>,
    Json(request): Json<CreateCloudLibraryRequest>,
) -> Result<Json<CloudLibraryStatus>, ApiError> {
    commands::create_cloud_library(request.confirmed, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/review-cloud-library-join",
    operation_id = "reviewCloudLibraryJoin",
    responses((status = 200, body = CloudLibraryJoinReview), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_review_cloud_library_join(
    State(state): State<HttpHostState>,
) -> Result<Json<CloudLibraryJoinReview>, ApiError> {
    commands::review_cloud_library_join(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/rebuild-cloud-library-from-local",
    operation_id = "rebuildCloudLibraryFromLocal",
    request_body = CreateCloudLibraryRequest,
    responses((status = 200, body = CloudLibraryStatus), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_rebuild_cloud_library_from_local(
    State(state): State<HttpHostState>,
    Json(request): Json<CreateCloudLibraryRequest>,
) -> Result<Json<CloudLibraryStatus>, ApiError> {
    commands::rebuild_cloud_library_from_local(request.confirmed, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/reconnect-cloud-library",
    operation_id = "reconnectCloudLibrary",
    request_body = CreateCloudLibraryRequest,
    responses((status = 200, body = CloudLibraryStatus), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_reconnect_cloud_library(
    State(state): State<HttpHostState>,
    Json(request): Json<CreateCloudLibraryRequest>,
) -> Result<Json<CloudLibraryStatus>, ApiError> {
    commands::reconnect_cloud_library(request.confirmed, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct JoinCloudLibraryRequest {
    pub decisions: Vec<JoinGameDecision>,
    pub confirmed_replacements: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/join-cloud-library",
    operation_id = "joinCloudLibrary",
    request_body = JoinCloudLibraryRequest,
    responses((status = 200, body = CloudLibraryJoinOutcome), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_join_cloud_library(
    State(state): State<HttpHostState>,
    Json(request): Json<JoinCloudLibraryRequest>,
) -> Result<Json<CloudLibraryJoinOutcome>, ApiError> {
    commands::join_cloud_library(
        request.decisions,
        request.confirmed_replacements,
        state.app().clone(),
    )
    .await
    .map(Json)
    .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/review-cloud-library-cutover",
    operation_id = "reviewCloudLibraryCutover",
    responses((status = 200, body = CloudLibraryCutoverReview), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_review_cloud_library_cutover(
    State(state): State<HttpHostState>,
) -> Result<Json<CloudLibraryCutoverReview>, ApiError> {
    commands::review_cloud_library_cutover(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CutoverCloudLibraryRequest {
    pub confirmed: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/cutover-cloud-library",
    operation_id = "cutoverCloudLibrary",
    request_body = CutoverCloudLibraryRequest,
    responses((status = 200, body = CloudLibraryCutoverOutcome), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_cutover_cloud_library(
    State(state): State<HttpHostState>,
    Json(request): Json<CutoverCloudLibraryRequest>,
) -> Result<Json<CloudLibraryCutoverOutcome>, ApiError> {
    commands::cutover_cloud_library(request.confirmed, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/get-cloud-archive-library",
    operation_id = "getCloudArchiveLibrary",
    responses((status = 200, body = CloudArchiveLibraryView), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_get_cloud_archive_library(
    State(state): State<HttpHostState>,
) -> Result<Json<CloudArchiveLibraryView>, ApiError> {
    commands::get_cloud_archive_library(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReviewV2GameProgressRequest {
    pub game_id: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/review-v2-game-progress",
    operation_id = "reviewV2GameProgress",
    request_body = ReviewV2GameProgressRequest,
    responses((status = 200, body = rgsm_core::cloud_sync::v2::V2ConflictReview), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_review_v2_game_progress(
    State(state): State<HttpHostState>,
    Json(request): Json<ReviewV2GameProgressRequest>,
) -> Result<Json<rgsm_core::cloud_sync::v2::V2ConflictReview>, ApiError> {
    commands::review_v2_game_progress(request.game_id, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct KeepV2LocalProgressRequest {
    pub game_id: String,
    pub manifest_revision: u64,
    pub local_snapshot_id: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/keep-v2-local-progress",
    operation_id = "keepV2LocalProgress",
    request_body = KeepV2LocalProgressRequest,
    responses((status = 200, body = rgsm_core::cloud_sync::v2::KeepLocalProgressOutcome), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_keep_v2_local_progress(
    State(state): State<HttpHostState>,
    Json(request): Json<KeepV2LocalProgressRequest>,
) -> Result<Json<rgsm_core::cloud_sync::v2::KeepLocalProgressOutcome>, ApiError> {
    commands::keep_v2_local_progress(
        request.game_id,
        request.manifest_revision,
        request.local_snapshot_id,
        state.app().clone(),
    )
    .await
    .map(Json)
    .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AcceptV2RemoteProgressRequest {
    pub game_id: String,
    pub manifest_revision: u64,
    pub expected_local_snapshot_id: Option<String>,
    pub selected_snapshot_id: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/accept-v2-remote-progress",
    operation_id = "acceptV2RemoteProgress",
    request_body = AcceptV2RemoteProgressRequest,
    responses((status = 200, body = rgsm_core::services::AcceptRemoteProgressOutcome), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_accept_v2_remote_progress(
    State(state): State<HttpHostState>,
    Json(request): Json<AcceptV2RemoteProgressRequest>,
) -> Result<Json<rgsm_core::services::AcceptRemoteProgressOutcome>, ApiError> {
    commands::accept_v2_remote_progress(
        request.game_id,
        request.manifest_revision,
        request.expected_local_snapshot_id,
        request.selected_snapshot_id,
        state.app().clone(),
    )
    .await
    .map(Json)
    .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/preview-materialize-all",
    operation_id = "previewMaterializeAll",
    responses((status = 200, body = MaterializationPreview), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_preview_materialize_all(
    State(state): State<HttpHostState>,
) -> Result<Json<MaterializationPreview>, ApiError> {
    commands::preview_materialize_all(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadCloudArchiveRequest {
    pub game_id: String,
    pub snapshot_id: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/upload-cloud-archive",
    operation_id = "uploadCloudArchive",
    request_body = UploadCloudArchiveRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_upload_cloud_archive(
    State(state): State<HttpHostState>,
    Json(request): Json<UploadCloudArchiveRequest>,
) -> Result<Json<()>, ApiError> {
    commands::upload_cloud_archive(request.game_id, request.snapshot_id, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DownloadCloudArchiveRequest {
    pub game_id: String,
    pub snapshot_id: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/download-cloud-archive",
    operation_id = "downloadCloudArchive",
    request_body = DownloadCloudArchiveRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_download_cloud_archive(
    State(state): State<HttpHostState>,
    Json(request): Json<DownloadCloudArchiveRequest>,
) -> Result<Json<()>, ApiError> {
    commands::download_cloud_archive(request.game_id, request.snapshot_id, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteV2SnapshotRequest {
    pub game_id: String,
    pub snapshot_id: String,
    pub confirmed: bool,
    #[serde(default)]
    pub current_position: Option<rgsm_core::services::CurrentPositionDecision>,
}

#[utoipa::path(
    post,
    path = "/api/v1/delete-v2-snapshot",
    operation_id = "deleteV2Snapshot",
    request_body = DeleteV2SnapshotRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_delete_v2_snapshot(
    State(state): State<HttpHostState>,
    Json(request): Json<DeleteV2SnapshotRequest>,
) -> Result<Json<()>, ApiError> {
    commands::delete_v2_snapshot(
        request.game_id,
        request.snapshot_id,
        request.confirmed,
        request.current_position,
        state.app().clone(),
    )
    .await
    .map(Json)
    .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetSharedSnapshotRetentionRequest {
    pub game_id: String,
    pub limit: Option<u32>,
    pub confirmed: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/set-shared-snapshot-retention",
    operation_id = "setSharedSnapshotRetention",
    request_body = SetSharedSnapshotRetentionRequest,
    responses((status = 200, body = rgsm_core::services::SnapshotRetentionOutcome), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_set_shared_snapshot_retention(
    State(state): State<HttpHostState>,
    Json(request): Json<SetSharedSnapshotRetentionRequest>,
) -> Result<Json<rgsm_core::services::SnapshotRetentionOutcome>, ApiError> {
    commands::set_shared_snapshot_retention(
        request.game_id,
        request.limit,
        request.confirmed,
        state.app().clone(),
    )
    .await
    .map(Json)
    .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetSnapshotRetentionProtectedRequest {
    pub game_id: String,
    pub snapshot_id: String,
    pub retention_protected: bool,
    pub confirmed: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/set-snapshot-retention-protected",
    operation_id = "setSnapshotRetentionProtected",
    request_body = SetSnapshotRetentionProtectedRequest,
    responses((status = 200, body = rgsm_core::services::SnapshotRetentionOutcome), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_set_snapshot_retention_protected(
    State(state): State<HttpHostState>,
    Json(request): Json<SetSnapshotRetentionProtectedRequest>,
) -> Result<Json<rgsm_core::services::SnapshotRetentionOutcome>, ApiError> {
    commands::set_snapshot_retention_protected(
        request.game_id,
        request.snapshot_id,
        request.retention_protected,
        request.confirmed,
        state.app().clone(),
    )
    .await
    .map(Json)
    .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/get-current-device-game-statuses",
    operation_id = "getCurrentDeviceGameStatuses",
    responses((status = 200, body = Vec<rgsm_core::services::DeviceGameStatus>), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_get_current_device_game_statuses(
    State(state): State<HttpHostState>,
) -> Result<Json<Vec<rgsm_core::services::DeviceGameStatus>>, ApiError> {
    commands::get_current_device_game_statuses(state.app().clone())
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetDeviceGameVisibilityRequest {
    pub game_id: String,
    pub visible: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/set-device-game-visibility",
    operation_id = "setDeviceGameVisibility",
    request_body = SetDeviceGameVisibilityRequest,
    responses((status = 200, body = rgsm_core::services::DeviceGameStatus), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_set_device_game_visibility(
    State(state): State<HttpHostState>,
    Json(request): Json<SetDeviceGameVisibilityRequest>,
) -> Result<Json<rgsm_core::services::DeviceGameStatus>, ApiError> {
    commands::set_device_game_visibility(request.game_id, request.visible, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetDeviceGameManagedRequest {
    pub game_id: String,
    pub managed: bool,
    pub confirmed: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/set-device-game-managed",
    operation_id = "setDeviceGameManaged",
    request_body = SetDeviceGameManagedRequest,
    responses((status = 200, body = rgsm_core::services::DeviceGameStatus), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_set_device_game_managed(
    State(state): State<HttpHostState>,
    Json(request): Json<SetDeviceGameManagedRequest>,
) -> Result<Json<rgsm_core::services::DeviceGameStatus>, ApiError> {
    commands::set_device_game_managed(
        request.game_id,
        request.managed,
        request.confirmed,
        state.app().clone(),
    )
    .await
    .map(Json)
    .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EvictLocalArchiveRequest {
    pub game_id: String,
    pub snapshot_id: String,
    pub confirmed: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/evict-local-archive",
    operation_id = "evictLocalArchive",
    request_body = EvictLocalArchiveRequest,
    responses((status = 200, body = bool), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_evict_local_archive(
    State(state): State<HttpHostState>,
    Json(request): Json<EvictLocalArchiveRequest>,
) -> Result<Json<bool>, ApiError> {
    commands::evict_local_archive(
        request.game_id,
        request.snapshot_id,
        request.confirmed,
        state.app().clone(),
    )
    .await
    .map(Json)
    .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EvictCloudArchiveRequest {
    pub game_id: String,
    pub snapshot_id: String,
    pub confirmed: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/evict-cloud-archive",
    operation_id = "evictCloudArchive",
    request_body = EvictCloudArchiveRequest,
    responses((status = 200, body = bool), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_evict_cloud_archive(
    State(state): State<HttpHostState>,
    Json(request): Json<EvictCloudArchiveRequest>,
) -> Result<Json<bool>, ApiError> {
    commands::evict_cloud_archive(
        request.game_id,
        request.snapshot_id,
        request.confirmed,
        state.app().clone(),
    )
    .await
    .map(Json)
    .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/get-cloud-device-profiles",
    operation_id = "getCloudDeviceProfiles",
    responses((status = 200, body = Vec<rgsm_core::services::CloudDeviceProfileView>), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_get_cloud_device_profiles(
    State(state): State<HttpHostState>,
) -> Result<Json<Vec<rgsm_core::services::CloudDeviceProfileView>>, ApiError> {
    commands::get_cloud_device_profiles(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemoveCloudDeviceProfileRequest {
    pub device_id: String,
    pub confirmed: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/remove-cloud-device-profile",
    operation_id = "removeCloudDeviceProfile",
    request_body = RemoveCloudDeviceProfileRequest,
    responses((status = 200, body = rgsm_core::cloud_sync::v2::DeviceProfileRemovalOutcome), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_remove_cloud_device_profile(
    State(state): State<HttpHostState>,
    Json(request): Json<RemoveCloudDeviceProfileRequest>,
) -> Result<Json<rgsm_core::cloud_sync::v2::DeviceProfileRemovalOutcome>, ApiError> {
    commands::remove_cloud_device_profile(request.device_id, request.confirmed, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/get-deleted-cloud-games",
    operation_id = "getDeletedCloudGames",
    responses((status = 200, body = Vec<rgsm_core::services::DeletedCloudGameView>), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_get_deleted_cloud_games(
    State(state): State<HttpHostState>,
) -> Result<Json<Vec<rgsm_core::services::DeletedCloudGameView>>, ApiError> {
    commands::get_deleted_cloud_games(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PermanentlyDeleteCloudGameRequest {
    pub game_id: String,
    pub confirmed: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/permanently-delete-cloud-game",
    operation_id = "permanentlyDeleteCloudGame",
    request_body = PermanentlyDeleteCloudGameRequest,
    responses((status = 200, body = rgsm_core::cloud_sync::v2::SharedGameDeletionOutcome), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_permanently_delete_cloud_game(
    State(state): State<HttpHostState>,
    Json(request): Json<PermanentlyDeleteCloudGameRequest>,
) -> Result<Json<rgsm_core::cloud_sync::v2::SharedGameDeletionOutcome>, ApiError> {
    commands::permanently_delete_cloud_game(request.game_id, request.confirmed, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/materialize-all-cloud-archives",
    operation_id = "materializeAllCloudArchives",
    responses((status = 200, body = MaterializationOutcome), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_materialize_all_cloud_archives(
    State(state): State<HttpHostState>,
) -> Result<Json<MaterializationOutcome>, ApiError> {
    commands::materialize_all_cloud_archives(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetGameSyncModeRequest {
    pub game_id: String,
    #[serde(default = "default_cloud_sync_enabled")]
    pub enabled: bool,
    pub mode: SyncMode,
    pub initial_catch_up: InitialCatchUpPolicy,
    pub live_save: Option<LiveSaveSyncOptions>,
}

fn default_cloud_sync_enabled() -> bool {
    true
}

#[utoipa::path(
    post,
    path = "/api/v1/set-game-sync-mode",
    operation_id = "setGameSyncMode",
    request_body = SetGameSyncModeRequest,
    responses((status = 200, body = GameSyncModeOutcome), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_set_game_sync_mode(
    State(state): State<HttpHostState>,
    Json(request): Json<SetGameSyncModeRequest>,
) -> Result<Json<GameSyncModeOutcome>, ApiError> {
    commands::set_game_sync_mode(
        request.game_id,
        request.enabled,
        request.mode,
        request.initial_catch_up,
        request.live_save,
        state.app().clone(),
    )
    .await
    .map(Json)
    .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CloudUploadAllRequest {
    pub session: CloudSyncSessionConfig,
}

#[utoipa::path(
    post,
    path = "/api/v1/cloud-upload-all",
    operation_id = "cloudUploadAll",
    request_body = CloudUploadAllRequest,
    responses((status = 200, body = BatchSyncReport), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_cloud_upload_all(
    State(state): State<HttpHostState>,
    Json(request): Json<CloudUploadAllRequest>,
) -> Result<Json<BatchSyncReport>, ApiError> {
    commands::cloud_upload_all(request.session, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CloudDownloadAllRequest {
    pub session: CloudSyncSessionConfig,
}

#[utoipa::path(
    post,
    path = "/api/v1/cloud-download-all",
    operation_id = "cloudDownloadAll",
    request_body = CloudDownloadAllRequest,
    responses((status = 200, body = BatchSyncReport), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_cloud_download_all(
    State(state): State<HttpHostState>,
    Json(request): Json<CloudDownloadAllRequest>,
) -> Result<Json<BatchSyncReport>, ApiError> {
    commands::cloud_download_all(request.session, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/cancel-cloud-sync",
    operation_id = "cancelCloudSync",
    responses((status = 200, body = CancelCloudSyncResult), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_cancel_cloud_sync(
    State(state): State<HttpHostState>,
) -> Result<Json<CancelCloudSyncResult>, ApiError> {
    commands::cancel_cloud_sync(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetSnapshotDescriptionRequest {
    pub game: Game,
    pub date: String,
    pub describe: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/set-snapshot-description",
    operation_id = "setSnapshotDescription",
    request_body = SetSnapshotDescriptionRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_set_snapshot_description(
    State(state): State<HttpHostState>,
    Json(request): Json<SetSnapshotDescriptionRequest>,
) -> Result<Json<()>, ApiError> {
    commands::set_snapshot_description(
        request.game,
        request.date,
        request.describe,
        state.app().clone(),
    )
    .await
    .map(Json)
    .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/backup-all",
    operation_id = "backupAll",
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_backup_all(State(state): State<HttpHostState>) -> Result<Json<()>, ApiError> {
    commands::backup_all(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/apply-all",
    operation_id = "applyAll",
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_apply_all(State(state): State<HttpHostState>) -> Result<Json<()>, ApiError> {
    commands::apply_all(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetQuickBackupGameRequest {
    pub game: Game,
}

#[utoipa::path(
    post,
    path = "/api/v1/set-quick-backup-game",
    operation_id = "setQuickBackupGame",
    request_body = SetQuickBackupGameRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_set_quick_backup_game(
    State(state): State<HttpHostState>,
    Json(request): Json<SetQuickBackupGameRequest>,
) -> Result<Json<()>, ApiError> {
    commands::set_quick_backup_game(state.app().clone(), request.game)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetGameAutoBackupRequest {
    pub game_name: String,
    pub auto_backup: Option<backup::AutoBackupConfig>,
}

#[utoipa::path(
    post,
    path = "/api/v1/set-game-auto-backup",
    operation_id = "setGameAutoBackup",
    request_body = SetGameAutoBackupRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_set_game_auto_backup(
    State(state): State<HttpHostState>,
    Json(request): Json<SetGameAutoBackupRequest>,
) -> Result<Json<()>, ApiError> {
    commands::set_game_auto_backup(state.app().clone(), request.game_name, request.auto_backup)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetGameAutomationRequest {
    pub storage_key: String,
    pub automation: Option<GameAutomationSettingsDraft>,
}

#[utoipa::path(
    post,
    path = "/api/v1/set-game-automation",
    operation_id = "setGameAutomation",
    request_body = SetGameAutomationRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_set_game_automation(
    State(state): State<HttpHostState>,
    Json(request): Json<SetGameAutomationRequest>,
) -> Result<Json<()>, ApiError> {
    commands::set_game_automation(state.app().clone(), request.storage_key, request.automation)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetGameAutoSaveSettingsRequest {
    pub storage_key: String,
    pub auto_backup: Option<backup::AutoBackupConfig>,
    pub automation: Option<GameAutomationSettingsDraft>,
}

#[utoipa::path(
    post,
    path = "/api/v1/set-game-auto-save-settings",
    operation_id = "setGameAutoSaveSettings",
    request_body = SetGameAutoSaveSettingsRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_set_game_auto_save_settings(
    State(state): State<HttpHostState>,
    Json(request): Json<SetGameAutoSaveSettingsRequest>,
) -> Result<Json<()>, ApiError> {
    commands::set_game_auto_save_settings(
        state.app().clone(),
        request.storage_key,
        request.auto_backup,
        request.automation,
    )
    .await
    .map(Json)
    .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetSnapshotCreatedByRequest {
    pub game_name: String,
    pub snapshot_date: String,
    pub created_by: CreatedBy,
}

#[utoipa::path(
    post,
    path = "/api/v1/set-snapshot-created-by",
    operation_id = "setSnapshotCreatedBy",
    request_body = SetSnapshotCreatedByRequest,
    responses((status = 200, body = GameSnapshots), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_set_snapshot_created_by(
    State(state): State<HttpHostState>,
    Json(request): Json<SetSnapshotCreatedByRequest>,
) -> Result<Json<GameSnapshots>, ApiError> {
    commands::set_snapshot_created_by(
        state.app().clone(),
        request.game_name,
        request.snapshot_date,
        request.created_by,
    )
    .await
    .map(Json)
    .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/get-auto-backup-status",
    operation_id = "getAutoBackupStatus",
    responses((status = 200, body = Vec<quick_actions::AutoBackupGameStatus>), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_get_auto_backup_status(
    State(state): State<HttpHostState>,
) -> Result<Json<Vec<quick_actions::AutoBackupGameStatus>>, ApiError> {
    commands::get_auto_backup_status(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/list-running-processes",
    operation_id = "listRunningProcesses",
    responses((status = 200, body = Vec<crate::process_util::RunningProcessOption>), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_list_running_processes()
-> Result<Json<Vec<crate::process_util::RunningProcessOption>>, ApiError> {
    commands::list_running_processes()
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolvePathRequest {
    pub path: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/resolve-path",
    operation_id = "resolvePath",
    request_body = ResolvePathRequest,
    responses((status = 200, body = String), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_resolve_path(
    Json(request): Json<ResolvePathRequest>,
) -> Result<Json<String>, ApiError> {
    commands::resolve_path(request.path)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/get-current-device-info",
    operation_id = "getCurrentDeviceInfo",
    responses((status = 200, body = Device), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_get_current_device_info() -> Result<Json<Device>, ApiError> {
    commands::get_current_device_info()
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToggleQuickActionSoundPreviewRequest {
    pub preferences: QuickActionSoundPreferences,
    pub effect: sound::QuickActionSoundEffect,
}

#[utoipa::path(
    post,
    path = "/api/v1/toggle-quick-action-sound-preview",
    operation_id = "toggleQuickActionSoundPreview",
    request_body = ToggleQuickActionSoundPreviewRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_toggle_quick_action_sound_preview(
    State(state): State<HttpHostState>,
    Json(request): Json<ToggleQuickActionSoundPreviewRequest>,
) -> Result<Json<()>, ApiError> {
    commands::toggle_quick_action_sound_preview(
        state.app().clone(),
        request.preferences,
        request.effect,
    )
    .await
    .map(Json)
    .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/stop-sound-playback",
    operation_id = "stopSoundPlayback",
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_stop_sound_playback(
    State(state): State<HttpHostState>,
) -> Result<Json<()>, ApiError> {
    commands::stop_sound_playback(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/choose-quick-action-sound-file",
    operation_id = "chooseQuickActionSoundFile",
    responses((status = 200, body = String), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_choose_quick_action_sound_file(
    State(state): State<HttpHostState>,
) -> Result<Json<String>, ApiError> {
    commands::choose_quick_action_sound_file(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetSnapshotHeadRequest {
    pub game: Game,
    pub date: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/set-snapshot-head",
    operation_id = "setSnapshotHead",
    request_body = SetSnapshotHeadRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_set_snapshot_head(
    State(state): State<HttpHostState>,
    Json(request): Json<SetSnapshotHeadRequest>,
) -> Result<Json<()>, ApiError> {
    commands::set_snapshot_head(request.game, request.date, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DetachSnapshotRequest {
    pub game: Game,
    pub date: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/detach-snapshot",
    operation_id = "detachSnapshot",
    request_body = DetachSnapshotRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_detach_snapshot(
    State(state): State<HttpHostState>,
    Json(request): Json<DetachSnapshotRequest>,
) -> Result<Json<()>, ApiError> {
    commands::detach_snapshot(request.game, request.date, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSnapshotAtRequest {
    pub game: Game,
    pub describe: String,
    pub parent_date: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/create-snapshot-at",
    operation_id = "createSnapshotAt",
    request_body = CreateSnapshotAtRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_create_snapshot_at(
    State(state): State<HttpHostState>,
    Json(request): Json<CreateSnapshotAtRequest>,
) -> Result<Json<()>, ApiError> {
    let window = state
        .app()
        .get_webview_window("main")
        .ok_or_else(|| ApiError::unavailable("Main window is not available"))?;
    commands::create_snapshot_at(
        request.game,
        request.describe,
        request.parent_date,
        window,
        state.app().clone(),
    )
    .await
    .map(Json)
    .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FetchLudusaviGamesRequest {
    pub filter_local_only: bool,
}

#[utoipa::path(
    post,
    path = "/api/v1/fetch-ludusavi-games",
    operation_id = "fetchLudusaviGames",
    request_body = FetchLudusaviGamesRequest,
    responses((status = 200, body = Vec<ImportableGame>), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_fetch_ludusavi_games(
    Json(request): Json<FetchLudusaviGamesRequest>,
) -> Result<Json<Vec<ImportableGame>>, ApiError> {
    commands::fetch_ludusavi_games(request.filter_local_only)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetGameSavePathsRequest {
    pub game_name: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/get-game-save-paths",
    operation_id = "getGameSavePaths",
    request_body = GetGameSavePathsRequest,
    responses((status = 200, body = Vec<SavePath>), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_get_game_save_paths(
    Json(request): Json<GetGameSavePathsRequest>,
) -> Result<Json<Vec<SavePath>>, ApiError> {
    commands::get_game_save_paths(request.game_name)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/get-path-placeholder-catalog",
    operation_id = "getPathPlaceholderCatalog",
    responses((status = 200, body = Vec<PathPlaceholderDescriptor>), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_get_path_placeholder_catalog()
-> Result<Json<Vec<PathPlaceholderDescriptor>>, ApiError> {
    Ok(Json(commands::get_path_placeholder_catalog()))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSaveUnitResolutionRequest {
    pub game: Game,
    pub save_unit: SaveUnit,
}

#[utoipa::path(
    post,
    path = "/api/v1/preview-save-unit-resolution",
    operation_id = "previewSaveUnitResolution",
    request_body = PreviewSaveUnitResolutionRequest,
    responses((status = 200, body = ResolutionReport), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_preview_save_unit_resolution(
    State(state): State<HttpHostState>,
    Json(request): Json<PreviewSaveUnitResolutionRequest>,
) -> Result<Json<ResolutionReport>, ApiError> {
    commands::preview_save_unit_resolution(request.game, request.save_unit, state.app().clone())
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetGameDeviceBindingRequest {
    pub identity: String,
    pub binding: GameDeviceBinding,
}

#[utoipa::path(
    post,
    path = "/api/v1/set-game-device-binding",
    operation_id = "setGameDeviceBinding",
    request_body = SetGameDeviceBindingRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_set_game_device_binding(
    State(state): State<HttpHostState>,
    Json(request): Json<SetGameDeviceBindingRequest>,
) -> Result<Json<()>, ApiError> {
    commands::set_game_device_binding(request.identity, request.binding, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SaveRestoreMappingRequest {
    pub identity: String,
    pub save_unit_id: u32,
    pub source_dimensions: rgsm_core::path_resolution::CandidateDimensions,
    pub target_candidate_ids: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/save-restore-mapping",
    operation_id = "saveRestoreMapping",
    request_body = SaveRestoreMappingRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_save_restore_mapping(
    State(state): State<HttpHostState>,
    Json(request): Json<SaveRestoreMappingRequest>,
) -> Result<Json<()>, ApiError> {
    commands::save_restore_mapping(
        request.identity,
        request.save_unit_id,
        request.source_dimensions,
        request.target_candidate_ids,
        state.app().clone(),
    )
    .await
    .map(Json)
    .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/get-ludusavi-manifest-status",
    operation_id = "getLudusaviManifestStatus",
    responses((status = 200, body = LudusaviManifestStatus), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_get_ludusavi_manifest_status() -> Result<Json<LudusaviManifestStatus>, ApiError> {
    commands::get_ludusavi_manifest_status()
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/update-ludusavi-manifest",
    operation_id = "updateLudusaviManifest",
    responses((status = 200, body = LudusaviManifestStatus), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_update_ludusavi_manifest() -> Result<Json<LudusaviManifestStatus>, ApiError> {
    commands::update_ludusavi_manifest()
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/reset-ludusavi-manifest-to-bundled",
    operation_id = "resetLudusaviManifestToBundled",
    responses((status = 200, body = LudusaviManifestStatus), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_reset_ludusavi_manifest_to_bundled()
-> Result<Json<LudusaviManifestStatus>, ApiError> {
    commands::reset_ludusavi_manifest_to_bundled()
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckPathsRequest {
    pub paths: Vec<String>,
    pub store_user_id: Option<String>,
    pub install_dirs: Option<Vec<String>>,
    pub steam_id: Option<u32>,
}

#[utoipa::path(
    post,
    path = "/api/v1/check-paths",
    operation_id = "checkPaths",
    request_body = CheckPathsRequest,
    responses((status = 200, body = Vec<path_resolver::PathCheckResult>), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_check_paths(
    State(state): State<HttpHostState>,
    Json(request): Json<CheckPathsRequest>,
) -> Result<Json<Vec<path_resolver::PathCheckResult>>, ApiError> {
    commands::check_paths(
        request.paths,
        request.store_user_id,
        request.install_dirs,
        request.steam_id,
        state.app().clone(),
    )
    .await
    .map(Json)
    .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/detect-game-roots",
    operation_id = "detectGameRoots",
    responses((status = 200, body = Vec<String>), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_detect_game_roots() -> Result<Json<Vec<String>>, ApiError> {
    commands::detect_game_roots()
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/detect-store-user-ids",
    operation_id = "detectStoreUserIds",
    responses((status = 200, body = Vec<steam::StoreUserIdCandidate>), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_detect_store_user_ids() -> Result<Json<Vec<steam::StoreUserIdCandidate>>, ApiError>
{
    commands::detect_store_user_ids()
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/get-system-fonts",
    operation_id = "getSystemFonts",
    responses((status = 200, body = Vec<String>), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_get_system_fonts() -> Result<Json<Vec<String>>, ApiError> {
    Ok(Json(commands::get_system_fonts()))
}

#[utoipa::path(
    post,
    path = "/api/v1/get-sync-state",
    operation_id = "getSyncState",
    responses((status = 200, body = cloud_sync::SyncState), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_get_sync_state() -> Result<Json<cloud_sync::SyncState>, ApiError> {
    commands::get_sync_state()
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScanVnsRequest {
    pub dirs: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/scan-vns",
    operation_id = "scanVns",
    request_body = ScanVnsRequest,
    responses((status = 200, body = Vec<GameDraft>), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_scan_vns(
    Json(request): Json<ScanVnsRequest>,
) -> Result<Json<Vec<GameDraft>>, ApiError> {
    commands::scan_vns(request.dirs)
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/list-config-backups",
    operation_id = "listConfigBackups",
    responses((status = 200, body = Vec<String>), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_list_config_backups() -> Result<Json<Vec<String>>, ApiError> {
    Ok(Json(commands::list_config_backups()))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestoreConfigBackupRequest {
    pub index: usize,
}

#[utoipa::path(
    post,
    path = "/api/v1/restore-config-backup",
    operation_id = "restoreConfigBackup",
    request_body = RestoreConfigBackupRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_restore_config_backup(
    State(state): State<HttpHostState>,
    Json(request): Json<RestoreConfigBackupRequest>,
) -> Result<Json<()>, ApiError> {
    commands::restore_config_backup(request.index, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SyncGameRequest {
    pub game_name: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/sync-game",
    operation_id = "syncGame",
    request_body = SyncGameRequest,
    responses((status = 200, body = SyncGameOutcome), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_sync_game(
    State(state): State<HttpHostState>,
    Json(request): Json<SyncGameRequest>,
) -> Result<Json<SyncGameOutcome>, ApiError> {
    commands::sync_game(request.game_name, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolveGameSyncConflictRequest {
    pub game_name: String,
    pub resolution: ConflictResolution,
}

#[utoipa::path(
    post,
    path = "/api/v1/resolve-game-sync-conflict",
    operation_id = "resolveGameSyncConflict",
    request_body = ResolveGameSyncConflictRequest,
    responses((status = 200, body = ConflictResolutionOutcome), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_resolve_game_sync_conflict(
    State(state): State<HttpHostState>,
    Json(request): Json<ResolveGameSyncConflictRequest>,
) -> Result<Json<ConflictResolutionOutcome>, ApiError> {
    commands::resolve_game_sync_conflict(request.game_name, request.resolution, state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post,
    path = "/api/v1/sync-config",
    operation_id = "syncConfig",
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_sync_config(State(state): State<HttpHostState>) -> Result<Json<()>, ApiError> {
    commands::sync_config(state.app().clone())
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HttpHostInfo {
    pub base_url: String,
    pub token: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/get-http-host-info",
    operation_id = "getHttpHostInfo",
    responses((status = 200, body = HttpHostInfo), (status = 401, body = ApiError))
)]
pub async fn http_get_http_host_info(State(state): State<HttpHostState>) -> Json<HttpHostInfo> {
    Json(HttpHostInfo {
        base_url: state.base_url().to_string(),
        token: state.api_token(),
    })
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RegenerateHttpApiTokenResponse {
    pub token: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/regenerate-http-api-token",
    operation_id = "regenerateHttpApiToken",
    responses((status = 200, body = RegenerateHttpApiTokenResponse), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn http_regenerate_http_api_token(
    State(state): State<HttpHostState>,
) -> Result<Json<RegenerateHttpApiTokenResponse>, ApiError> {
    Ok(Json(RegenerateHttpApiTokenResponse {
        token: state.regenerate_api_token()?,
    }))
}

pub fn router() -> Router<HttpHostState> {
    Router::new()
        .route("/api/v1/open-url", post(http_open_url))
        .route("/api/v1/get-build-info", post(http_get_build_info))
        .route("/api/v1/get-http-host-info", post(http_get_http_host_info))
        .route(
            "/api/v1/regenerate-http-api-token",
            post(http_regenerate_http_api_token),
        )
        .route(
            "/api/v1/open-file-or-folder",
            post(http_open_file_or_folder),
        )
        .route("/api/v1/get-app-log-dir", post(http_get_app_log_dir))
        .route("/api/v1/choose-save-file", post(http_choose_save_file))
        .route("/api/v1/choose-save-dir", post(http_choose_save_dir))
        .route("/api/v1/get-local-config", post(http_get_local_config))
        .route("/api/v1/add-game", post(http_add_game))
        .route("/api/v1/update-game", post(http_update_game))
        .route("/api/v1/restore-snapshot", post(http_restore_snapshot))
        .route("/api/v1/delete-snapshot", post(http_delete_snapshot))
        .route(
            "/api/v1/batch-delete-snapshots",
            post(http_batch_delete_snapshots),
        )
        .route(
            "/api/v1/get-cloud-namespace-generation",
            post(http_get_cloud_namespace_generation),
        )
        .route("/api/v1/delete-game", post(http_delete_game))
        .route(
            "/api/v1/get-game-snapshots-info",
            post(http_get_game_snapshots_info),
        )
        .route(
            "/api/v1/verify-archive-integrity",
            post(http_verify_archive_integrity),
        )
        .route("/api/v1/set-config", post(http_set_config))
        .route("/api/v1/reset-settings", post(http_reset_settings))
        .route("/api/v1/create-snapshot", post(http_create_snapshot))
        .route("/api/v1/open-backup-folder", post(http_open_backup_folder))
        .route(
            "/api/v1/get-game-extra-backups",
            post(http_get_game_extra_backups),
        )
        .route(
            "/api/v1/delete-extra-backup",
            post(http_delete_extra_backup),
        )
        .route(
            "/api/v1/restore-extra-backup",
            post(http_restore_extra_backup),
        )
        .route(
            "/api/v1/open-extra-backup-folder",
            post(http_open_extra_backup_folder),
        )
        .route(
            "/api/v1/check-cloud-backend",
            post(http_check_cloud_backend),
        )
        .route(
            "/api/v1/inspect-cloud-library",
            post(http_inspect_cloud_library),
        )
        .route(
            "/api/v1/create-cloud-library",
            post(http_create_cloud_library),
        )
        .route(
            "/api/v1/rebuild-cloud-library-from-local",
            post(http_rebuild_cloud_library_from_local),
        )
        .route(
            "/api/v1/reconnect-cloud-library",
            post(http_reconnect_cloud_library),
        )
        .route(
            "/api/v1/review-cloud-library-join",
            post(http_review_cloud_library_join),
        )
        .route("/api/v1/join-cloud-library", post(http_join_cloud_library))
        .route(
            "/api/v1/review-cloud-library-cutover",
            post(http_review_cloud_library_cutover),
        )
        .route(
            "/api/v1/cutover-cloud-library",
            post(http_cutover_cloud_library),
        )
        .route(
            "/api/v1/get-cloud-archive-library",
            post(http_get_cloud_archive_library),
        )
        .route(
            "/api/v1/review-v2-game-progress",
            post(http_review_v2_game_progress),
        )
        .route(
            "/api/v1/keep-v2-local-progress",
            post(http_keep_v2_local_progress),
        )
        .route(
            "/api/v1/accept-v2-remote-progress",
            post(http_accept_v2_remote_progress),
        )
        .route(
            "/api/v1/preview-materialize-all",
            post(http_preview_materialize_all),
        )
        .route(
            "/api/v1/upload-cloud-archive",
            post(http_upload_cloud_archive),
        )
        .route(
            "/api/v1/download-cloud-archive",
            post(http_download_cloud_archive),
        )
        .route("/api/v1/delete-v2-snapshot", post(http_delete_v2_snapshot))
        .route(
            "/api/v1/set-shared-snapshot-retention",
            post(http_set_shared_snapshot_retention),
        )
        .route(
            "/api/v1/set-snapshot-retention-protected",
            post(http_set_snapshot_retention_protected),
        )
        .route(
            "/api/v1/get-current-device-game-statuses",
            post(http_get_current_device_game_statuses),
        )
        .route(
            "/api/v1/set-device-game-visibility",
            post(http_set_device_game_visibility),
        )
        .route(
            "/api/v1/set-device-game-managed",
            post(http_set_device_game_managed),
        )
        .route(
            "/api/v1/evict-local-archive",
            post(http_evict_local_archive),
        )
        .route(
            "/api/v1/evict-cloud-archive",
            post(http_evict_cloud_archive),
        )
        .route(
            "/api/v1/get-cloud-device-profiles",
            post(http_get_cloud_device_profiles),
        )
        .route(
            "/api/v1/remove-cloud-device-profile",
            post(http_remove_cloud_device_profile),
        )
        .route(
            "/api/v1/get-deleted-cloud-games",
            post(http_get_deleted_cloud_games),
        )
        .route(
            "/api/v1/permanently-delete-cloud-game",
            post(http_permanently_delete_cloud_game),
        )
        .route(
            "/api/v1/materialize-all-cloud-archives",
            post(http_materialize_all_cloud_archives),
        )
        .route("/api/v1/set-game-sync-mode", post(http_set_game_sync_mode))
        .route("/api/v1/cloud-upload-all", post(http_cloud_upload_all))
        .route("/api/v1/cloud-download-all", post(http_cloud_download_all))
        .route("/api/v1/cancel-cloud-sync", post(http_cancel_cloud_sync))
        .route(
            "/api/v1/set-snapshot-description",
            post(http_set_snapshot_description),
        )
        .route("/api/v1/backup-all", post(http_backup_all))
        .route("/api/v1/apply-all", post(http_apply_all))
        .route(
            "/api/v1/set-quick-backup-game",
            post(http_set_quick_backup_game),
        )
        .route(
            "/api/v1/set-game-auto-backup",
            post(http_set_game_auto_backup),
        )
        .route(
            "/api/v1/set-game-automation",
            post(http_set_game_automation),
        )
        .route(
            "/api/v1/set-game-auto-save-settings",
            post(http_set_game_auto_save_settings),
        )
        .route(
            "/api/v1/set-snapshot-created-by",
            post(http_set_snapshot_created_by),
        )
        .route(
            "/api/v1/get-auto-backup-status",
            post(http_get_auto_backup_status),
        )
        .route(
            "/api/v1/list-running-processes",
            post(http_list_running_processes),
        )
        .route("/api/v1/resolve-path", post(http_resolve_path))
        .route(
            "/api/v1/get-current-device-info",
            post(http_get_current_device_info),
        )
        .route(
            "/api/v1/toggle-quick-action-sound-preview",
            post(http_toggle_quick_action_sound_preview),
        )
        .route(
            "/api/v1/stop-sound-playback",
            post(http_stop_sound_playback),
        )
        .route(
            "/api/v1/choose-quick-action-sound-file",
            post(http_choose_quick_action_sound_file),
        )
        .route("/api/v1/set-snapshot-head", post(http_set_snapshot_head))
        .route("/api/v1/detach-snapshot", post(http_detach_snapshot))
        .route("/api/v1/create-snapshot-at", post(http_create_snapshot_at))
        .route(
            "/api/v1/fetch-ludusavi-games",
            post(http_fetch_ludusavi_games),
        )
        .route(
            "/api/v1/get-game-save-paths",
            post(http_get_game_save_paths),
        )
        .route(
            "/api/v1/get-path-placeholder-catalog",
            post(http_get_path_placeholder_catalog),
        )
        .route(
            "/api/v1/preview-save-unit-resolution",
            post(http_preview_save_unit_resolution),
        )
        .route(
            "/api/v1/set-game-device-binding",
            post(http_set_game_device_binding),
        )
        .route(
            "/api/v1/save-restore-mapping",
            post(http_save_restore_mapping),
        )
        .route(
            "/api/v1/get-ludusavi-manifest-status",
            post(http_get_ludusavi_manifest_status),
        )
        .route(
            "/api/v1/update-ludusavi-manifest",
            post(http_update_ludusavi_manifest),
        )
        .route(
            "/api/v1/reset-ludusavi-manifest-to-bundled",
            post(http_reset_ludusavi_manifest_to_bundled),
        )
        .route("/api/v1/check-paths", post(http_check_paths))
        .route("/api/v1/detect-game-roots", post(http_detect_game_roots))
        .route(
            "/api/v1/detect-store-user-ids",
            post(http_detect_store_user_ids),
        )
        .route("/api/v1/get-system-fonts", post(http_get_system_fonts))
        .route("/api/v1/get-sync-state", post(http_get_sync_state))
        .route("/api/v1/scan-vns", post(http_scan_vns))
        .route(
            "/api/v1/list-config-backups",
            post(http_list_config_backups),
        )
        .route(
            "/api/v1/restore-config-backup",
            post(http_restore_config_backup),
        )
        .route("/api/v1/sync-game", post(http_sync_game))
        .route(
            "/api/v1/resolve-game-sync-conflict",
            post(http_resolve_game_sync_conflict),
        )
        .route("/api/v1/sync-config", post(http_sync_config))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        http_open_url,
        http_get_build_info,
        http_open_file_or_folder,
        http_get_app_log_dir,
        http_choose_save_file,
        http_choose_save_dir,
        http_get_local_config,
        http_add_game,
        http_update_game,
        http_restore_snapshot,
        http_delete_snapshot,
        http_batch_delete_snapshots,
        http_get_cloud_namespace_generation,
        http_delete_game,
        http_get_game_snapshots_info,
        http_verify_archive_integrity,
        http_set_config,
        http_reset_settings,
        http_create_snapshot,
        http_open_backup_folder,
        http_get_game_extra_backups,
        http_delete_extra_backup,
        http_restore_extra_backup,
        http_open_extra_backup_folder,
        http_check_cloud_backend,
        http_inspect_cloud_library,
        http_create_cloud_library,
        http_rebuild_cloud_library_from_local,
        http_reconnect_cloud_library,
        http_review_cloud_library_join,
        http_join_cloud_library,
        http_review_cloud_library_cutover,
        http_cutover_cloud_library,
        http_get_cloud_archive_library,
        http_review_v2_game_progress,
        http_keep_v2_local_progress,
        http_accept_v2_remote_progress,
        http_preview_materialize_all,
        http_upload_cloud_archive,
        http_download_cloud_archive,
        http_delete_v2_snapshot,
        http_set_shared_snapshot_retention,
        http_set_snapshot_retention_protected,
        http_get_current_device_game_statuses,
        http_set_device_game_visibility,
        http_set_device_game_managed,
        http_evict_local_archive,
        http_evict_cloud_archive,
        http_get_cloud_device_profiles,
        http_remove_cloud_device_profile,
        http_get_deleted_cloud_games,
        http_permanently_delete_cloud_game,
        http_materialize_all_cloud_archives,
        http_set_game_sync_mode,
        http_cloud_upload_all,
        http_cloud_download_all,
        http_cancel_cloud_sync,
        http_set_snapshot_description,
        http_backup_all,
        http_apply_all,
        http_set_quick_backup_game,
        http_set_game_auto_backup,
        http_set_game_automation,
        http_set_game_auto_save_settings,
        http_set_snapshot_created_by,
        http_get_auto_backup_status,
        http_list_running_processes,
        http_resolve_path,
        http_get_current_device_info,
        http_toggle_quick_action_sound_preview,
        http_stop_sound_playback,
        http_choose_quick_action_sound_file,
        http_set_snapshot_head,
        http_detach_snapshot,
        http_create_snapshot_at,
        http_fetch_ludusavi_games,
        http_get_game_save_paths,
        http_get_path_placeholder_catalog,
        http_preview_save_unit_resolution,
        http_set_game_device_binding,
        http_save_restore_mapping,
        http_get_ludusavi_manifest_status,
        http_update_ludusavi_manifest,
        http_reset_ludusavi_manifest_to_bundled,
        http_check_paths,
        http_detect_game_roots,
        http_detect_store_user_ids,
        http_get_system_fonts,
        http_get_sync_state,
        http_scan_vns,
        http_list_config_backups,
        http_restore_config_backup,
        http_sync_game,
        http_resolve_game_sync_conflict,
        http_sync_config,
        http_get_http_host_info,
        http_regenerate_http_api_token,
        crate::http::stream_events,
    ),
    components(schemas(
        ApiError,
        crate::http::ApiErrorCode,
        HttpHostInfo,
        RegenerateHttpApiTokenResponse,
        crate::http::HostEvent,
        crate::commands::HostNotification,
        crate::commands::CloudSyncStatusEvent,
        crate::commands::CloudSyncErrorEvent,
        crate::quick_actions::QuickActionCompleted,
        OpenUrlRequest,
        OpenFileOrFolderRequest,
        AddGameRequest,
        UpdateGameRequest,
        RestoreSnapshotRequest,
        DeleteSnapshotRequest,
        BatchDeleteSnapshotsRequest,
        DeleteGameRequest,
        GetGameSnapshotsInfoRequest,
        VerifyArchiveIntegrityRequest,
        SetConfigRequest,
        CreateSnapshotRequest,
        OpenBackupFolderRequest,
        GetGameExtraBackupsRequest,
        DeleteExtraBackupRequest,
        RestoreExtraBackupRequest,
        OpenExtraBackupFolderRequest,
        CheckCloudBackendRequest,
        CreateCloudLibraryRequest,
        JoinCloudLibraryRequest,
        CutoverCloudLibraryRequest,
        ReviewV2GameProgressRequest,
        KeepV2LocalProgressRequest,
        AcceptV2RemoteProgressRequest,
        UploadCloudArchiveRequest,
        DownloadCloudArchiveRequest,
        DeleteV2SnapshotRequest,
        SetSharedSnapshotRetentionRequest,
        SetSnapshotRetentionProtectedRequest,
        SetDeviceGameVisibilityRequest,
        SetDeviceGameManagedRequest,
        EvictLocalArchiveRequest,
        EvictCloudArchiveRequest,
        RemoveCloudDeviceProfileRequest,
        PermanentlyDeleteCloudGameRequest,
        SetGameSyncModeRequest,
        CloudUploadAllRequest,
        CloudDownloadAllRequest,
        SetSnapshotDescriptionRequest,
        SetQuickBackupGameRequest,
        SetGameAutoBackupRequest,
        SetGameAutomationRequest,
        SetGameAutoSaveSettingsRequest,
        SetSnapshotCreatedByRequest,
        ResolvePathRequest,
        ToggleQuickActionSoundPreviewRequest,
        SetSnapshotHeadRequest,
        DetachSnapshotRequest,
        CreateSnapshotAtRequest,
        FetchLudusaviGamesRequest,
        GetGameSavePathsRequest,
        PreviewSaveUnitResolutionRequest,
        SetGameDeviceBindingRequest,
        SaveRestoreMappingRequest,
        CheckPathsRequest,
        ScanVnsRequest,
        RestoreConfigBackupRequest,
        SyncGameRequest,
        ResolveGameSyncConflictRequest,
        RestoreError,
    ))
)]
pub struct ApiDoc;
