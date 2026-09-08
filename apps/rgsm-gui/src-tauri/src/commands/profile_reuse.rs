use crate::{
    hooks::HookPipelineState,
    http::{ApiError, HttpHostState},
};
use axum::{Json, extract::State};
use rgsm_core::{
    config::get_config,
    services::{ProfileReuseError, ServiceContext},
};
use serde::Deserialize;
use tauri::Manager;

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReuseLocationsRequest {
    pub device_id: String,
    #[serde(default)]
    pub confirmed: bool,
}

#[utoipa::path(post, path = "/api/v1/reuse-cloud-device-locations", operation_id = "reuseCloudDeviceLocations",
    request_body = ReuseLocationsRequest, responses((status = 200, body = usize), (status = 400, body = ApiError)))]
pub async fn reuse(
    State(state): State<HttpHostState>,
    Json(request): Json<ReuseLocationsRequest>,
) -> Result<Json<usize>, ApiError> {
    if !request.confirmed {
        return Err(ApiError::from_command(
            ProfileReuseError::ConfirmationRequired,
        ));
    }
    crate::cloud_operation::run_after_cancelling(state.app(), async {
        let count = ServiceContext::new(state.app().state::<HookPipelineState>().snapshot())
            .reuse_cloud_device_locations(&request.device_id, true)
            .await
            .map_err(ApiError::from_command)?;
        let config = get_config().map_err(ApiError::from_command)?;
        crate::hooks::rebuild_pipeline(state.app(), &config);
        Ok(Json(count))
    })
    .await
}
