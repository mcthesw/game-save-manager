use crate::{
    hooks::HookPipelineState,
    http::{ApiError, HttpHostState},
};
use axum::{Json, Router, extract::State, http::header, response::IntoResponse, routing::post};
use rgsm_core::{
    config::get_config,
    services::{CloudJoinPreview, CloudLibraryStatus, ServiceContext},
};
use serde::Deserialize;
use tauri::Manager;
use utoipa::ToSchema;

// Do not derive Debug: this request carries cloud credentials.
#[derive(Deserialize, ToSchema)]
pub struct JoinCodeRequest {
    pub code: String,
    #[serde(default)]
    pub confirmed: bool,
}

#[utoipa::path(post, path = "/api/v1/export-cloud-join-code", operation_id = "exportCloudJoinCode",
    responses((status = 200, body = String), (status = 400, body = ApiError)))]
pub async fn export(State(state): State<HttpHostState>) -> Result<impl IntoResponse, ApiError> {
    let code = ServiceContext::new(state.app().state::<HookPipelineState>().snapshot())
        .export_cloud_join_code()
        .await
        .map_err(ApiError::from_command)?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(code)))
}

#[utoipa::path(post, path = "/api/v1/preview-cloud-join-code", operation_id = "previewCloudJoinCode",
    request_body = JoinCodeRequest, responses((status = 200, body = CloudJoinPreview), (status = 400, body = ApiError)))]
pub async fn preview(
    State(state): State<HttpHostState>,
    Json(request): Json<JoinCodeRequest>,
) -> Result<Json<CloudJoinPreview>, ApiError> {
    ServiceContext::new(state.app().state::<HookPipelineState>().snapshot())
        .preview_cloud_join_code(&request.code)
        .await
        .map(Json)
        .map_err(ApiError::from_command)
}

#[utoipa::path(post, path = "/api/v1/import-cloud-join-code", operation_id = "importCloudJoinCode",
    request_body = JoinCodeRequest, responses((status = 200, body = CloudLibraryStatus), (status = 400, body = ApiError)))]
pub async fn import(
    State(state): State<HttpHostState>,
    Json(request): Json<JoinCodeRequest>,
) -> Result<Json<CloudLibraryStatus>, ApiError> {
    if !request.confirmed {
        return Err(ApiError::from_command(
            rgsm_core::services::CloudJoinError::ConfirmationRequired,
        ));
    }
    crate::cloud_operation::run_after_cancelling(state.app(), async {
        let status = ServiceContext::new(state.app().state::<HookPipelineState>().snapshot())
            .import_cloud_join_code(&request.code, request.confirmed)
            .await
            .map_err(ApiError::from_command)?;
        let config = get_config().map_err(ApiError::from_command)?;
        crate::hooks::rebuild_pipeline(state.app(), &config);
        Ok(Json(status))
    })
    .await
}

pub fn router() -> Router<HttpHostState> {
    Router::new()
        .route("/api/v1/export-cloud-join-code", post(export))
        .route("/api/v1/preview-cloud-join-code", post(preview))
        .route("/api/v1/import-cloud-join-code", post(import))
}
