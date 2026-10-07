use axum::Json;
use rgsm_core::{backup::local_upgrade::LocalUpgradeView, services::LocalUpgradeAction};

use crate::http::ApiError;

#[utoipa::path(
    post, path = "/api/v1/local-archive-upgrade", operation_id = "localArchiveUpgrade",
    request_body = LocalUpgradeAction,
    responses((status = 200, body = LocalUpgradeView), (status = 500, body = ApiError))
)]
pub async fn http_local_archive_upgrade(
    Json(action): Json<LocalUpgradeAction>,
) -> Result<Json<LocalUpgradeView>, ApiError> {
    tokio::task::spawn_blocking(move || rgsm_core::services::local_archive_upgrade(action))
        .await
        .map_err(|error| ApiError::from_command(error.to_string()))?
        .map(Json)
        .map_err(|error| ApiError::from_command(error.to_string()))
}
