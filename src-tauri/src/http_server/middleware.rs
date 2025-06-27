use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
};
use log::error;

use crate::config::get_config;
use crate::http_server::models::AppState;

// API key middleware
pub async fn verify_api_key(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<(), (StatusCode, &'static str)> {
    let api_key = headers
        .get("X-API-Key")
        .ok_or((StatusCode::UNAUTHORIZED, "API key is required"))?
        .to_str()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid API key format"))?;

    let config = get_config().map_err(|_| {
        error!(target: "rgsm::http_server", "Failed to get application config");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to get application config",
        )
    })?;

    if api_key != config.settings.http_server.api_key {
        return Err((StatusCode::UNAUTHORIZED, "Invalid API key"));
    }

    Ok(())
}