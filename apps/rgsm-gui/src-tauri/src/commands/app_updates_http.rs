use axum::{Json, Router, extract::State, routing::post};

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

pub fn router() -> Router<HttpHostState> {
    Router::new().route("/api/v1/check-app-update", post(check))
}
