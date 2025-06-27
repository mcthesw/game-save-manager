use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use log::info;

use crate::config::get_config;
use crate::http_server::middleware::verify_api_key;
use crate::http_server::models::{
    AddGameRequest, ApiResponse, AppState, CreateSnapshotRequest, DeleteSnapshotRequest,
    RestoreSnapshotRequest,
};
use crate::backup::{Game, GameSnapshots};

// API handlers
pub async fn get_games() -> impl IntoResponse {
    match get_config() {
        Ok(config) => Json(ApiResponse::success(config.games)),
        Err(e) => Json(ApiResponse::<Vec<Game>>::error(e.to_string())),
    }
}

pub async fn get_game_snapshots(Path(game_name): Path<String>) -> impl IntoResponse {
    let config = match get_config() {
        Ok(config) => config,
        Err(e) => return Json(ApiResponse::<GameSnapshots>::error(e.to_string())),
    };

    let game = match config.games.iter().find(|g| g.name == game_name) {
        Some(game) => game.clone(),
        None => {
            return Json(ApiResponse::<GameSnapshots>::error(format!(
                "Game '{}' not found",
                game_name
            )))
        }
    };

    match game.get_game_snapshots_info() {
        Ok(snapshots) => Json(ApiResponse::success(snapshots)),
        Err(e) => Json(ApiResponse::<GameSnapshots>::error(e.to_string())),
    }
}

pub async fn create_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateSnapshotRequest>,
) -> impl IntoResponse {
    if let Err(err) = verify_api_key(headers, State(state.clone())).await {
        return err.into_response();
    }

    info!(
        target: "rgsm::http_server",
        "Creating snapshot for game: {:?}", request.game
    );

    match request.game.create_snapshot(&request.description).await {
        Ok(_) => Json(ApiResponse::<()>::success(())).into_response(),
        Err(e) => Json(ApiResponse::<()>::error(e.to_string())).into_response(),
    }
}

pub async fn restore_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RestoreSnapshotRequest>,
) -> impl IntoResponse {
    if let Err(err) = verify_api_key(headers, State(state.clone())).await {
        return err.into_response();
    }

    info!(
        target: "rgsm::http_server",
        "Restoring snapshot for game: {:?}, date: {}", request.game, request.date
    );

    match request.game.restore_snapshot(&request.date, Some(&state.app_handle)) {
        Ok(_) => Json(ApiResponse::<()>::success(())).into_response(),
        Err(e) => Json(ApiResponse::<()>::error(e.to_string())).into_response(),
    }
}

pub async fn delete_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<DeleteSnapshotRequest>,
) -> impl IntoResponse {
    if let Err(err) = verify_api_key(headers, State(state.clone())).await {
        return err.into_response();
    }

    info!(
        target: "rgsm::http_server",
        "Deleting snapshot for game: {:?}, date: {}", request.game, request.date
    );

    match request.game.delete_snapshot(&request.date) {
        Ok(_) => Json(ApiResponse::<()>::success(())).into_response(),
        Err(e) => Json(ApiResponse::<()>::error(e.to_string())).into_response(),
    }
}

pub async fn add_game(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AddGameRequest>,
) -> impl IntoResponse {
    if let Err(err) = verify_api_key(headers, State(state.clone())).await {
        return err.into_response();
    }

    info!(
        target: "rgsm::http_server",
        "Adding new game: {:?}", request.game
    );

    let mut config = match get_config() {
        Ok(config) => config,
        Err(e) => return Json(ApiResponse::<()>::error(e.to_string())).into_response(),
    };

    // Check if game already exists
    if config.games.iter().any(|g| g.name == request.game.name) {
        return Json(ApiResponse::<()>::error(format!(
            "Game '{}' already exists",
            request.game.name
        )))
        .into_response();
    }

    // Add the game
    config.games.push(request.game);

    // Save the config
    match crate::config::set_config(&config).await {
        Ok(_) => Json(ApiResponse::<()>::success(())).into_response(),
        Err(e) => Json(ApiResponse::<()>::error(e.to_string())).into_response(),
    }
}