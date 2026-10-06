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
