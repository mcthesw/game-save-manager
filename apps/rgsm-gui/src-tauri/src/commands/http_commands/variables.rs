use super::*;

#[derive(Debug, Deserialize, ToSchema)]
pub struct VariableSetupRequest {
    pub game: GameDraft,
}

#[utoipa::path(post, path = "/api/v1/missing-game-variables", operation_id = "missingGameVariables",
    request_body = VariableSetupRequest,
    responses((status = 200, body = Vec<String>), (status = 500, body = ApiError)))]
pub async fn http_missing_game_variables(
    Json(request): Json<VariableSetupRequest>,
) -> Result<Json<Vec<String>>, ApiError> {
    let config = get_config().map_err(|e| ApiError::from_command(e.to_string()))?;
    Ok(Json(rgsm_core::services::missing_game_variables(
        &config,
        &request.game.into_game(None),
        rgsm_core::device::get_current_device_id(),
    )))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverVariableRequest {
    pub game: GameDraft,
    pub paths: Vec<String>,
    pub name: String,
    #[serde(default)]
    pub device_variables: rgsm_core::services::DeviceVariableEdits,
}

#[utoipa::path(post, path = "/api/v1/discover-path-variable", operation_id = "discoverPathVariable",
    request_body = DiscoverVariableRequest,
    responses((status = 200, body = rgsm_core::path_variables::VariableDiscovery), (status = 500, body = ApiError)))]
pub async fn http_discover_path_variable(
    Json(request): Json<DiscoverVariableRequest>,
) -> Result<Json<rgsm_core::path_variables::VariableDiscovery>, ApiError> {
    let config = get_config().map_err(|e| ApiError::from_command(e.to_string()))?;
    tokio::task::spawn_blocking(move || {
        rgsm_core::services::discover_game_variable(
            &config,
            &request.game.into_game(None),
            &request.paths,
            &request.name,
            &request.device_variables,
        )
    })
    .await
    .map_err(|e| ApiError::from_command(e.to_string()))?
    .map(Json)
    .map_err(|e| ApiError::from_command(e.to_string()))
}
