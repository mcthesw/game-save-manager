use axum::{Json, Router, extract::State, routing::post};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    app_updates::{self, UpdateCheck},
    http::{ApiError, HttpHostState},
};

#[utoipa::path(
    post,
    path = "/api/v1/check-app-update",
    operation_id = "checkAppUpdate",
    responses((status = 200, body = UpdateCheck), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn check(State(state): State<HttpHostState>) -> Result<Json<UpdateCheck>, ApiError> {
    app_updates::check(state.app())
        .await
        .map(Json)
        .map_err(ApiError::from_display)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallUpdateRequest {
    pub expected_version: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/install-app-update",
    operation_id = "installAppUpdate",
    request_body = InstallUpdateRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError), (status = 401, body = ApiError), (status = 500, body = ApiError))
)]
pub async fn install(
    State(state): State<HttpHostState>,
    Json(request): Json<InstallUpdateRequest>,
) -> Result<Json<()>, ApiError> {
    app_updates::install(state.app().clone(), request.expected_version)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post, path = "/api/v1/download-app-update", operation_id = "downloadAppUpdate",
    request_body = InstallUpdateRequest,
    responses((status = 200, body = ()), (status = 400, body = ApiError))
)]
pub async fn download(
    State(state): State<HttpHostState>,
    Json(request): Json<InstallUpdateRequest>,
) -> Result<Json<()>, ApiError> {
    app_updates::download(state.app().clone(), request.expected_version)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(
    post, path = "/api/v1/get-app-update-state", operation_id = "getAppUpdateState",
    responses((status = 200, body = app_updates::UpdateProgress))
)]
pub async fn status(State(state): State<HttpHostState>) -> Json<app_updates::UpdateProgress> {
    Json(app_updates::snapshot(state.app()))
}

#[utoipa::path(
    post, path = "/api/v1/cancel-app-update-install", operation_id = "cancelAppUpdateInstall",
    responses((status = 200, body = ()))
)]
pub async fn cancel(State(state): State<HttpHostState>) -> Json<()> {
    app_updates::cancel_install(state.app());
    Json(())
}

pub fn router() -> Router<HttpHostState> {
    Router::new()
        .route("/api/v1/check-app-update", post(check))
        .route("/api/v1/install-app-update", post(install))
        .route("/api/v1/download-app-update", post(download))
        .route("/api/v1/get-app-update-state", post(status))
        .route("/api/v1/cancel-app-update-install", post(cancel))
}
