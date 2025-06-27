use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use log::{error, info};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tower_http::cors::{Any, CorsLayer};

use crate::backup::{Game, GameSnapshots};
use crate::config::get_config;
use crate::preclude::*;

// API key middleware
async fn verify_api_key(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<(), (StatusCode, &'static str)> {
    let api_key = headers
        .get("X-API-Key")
        .ok_or((StatusCode::UNAUTHORIZED, "API key is required"))?
        .to_str()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid API key format"))?;

    let config = get_config().map_err(|_| {
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

// Application state
#[derive(Clone)]
struct AppState {
    app_handle: Arc<AppHandle>,
}

// Response types
#[derive(Serialize)]
struct ApiResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

impl<T> ApiResponse<T> {
    fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    fn error(error: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
        }
    }
}

// Request types
#[derive(Deserialize)]
struct CreateSnapshotRequest {
    game: Game,
    description: String,
}

#[derive(Deserialize)]
struct RestoreSnapshotRequest {
    game: Game,
    date: String,
}

#[derive(Deserialize)]
struct DeleteSnapshotRequest {
    game: Game,
    date: String,
}

#[derive(Deserialize)]
struct AddGameRequest {
    game: Game,
}

// API handlers
async fn get_games() -> impl IntoResponse {
    match get_config() {
        Ok(config) => Json(ApiResponse::success(config.games)),
        Err(e) => Json(ApiResponse::<Vec<Game>>::error(e.to_string())),
    }
}

async fn get_game_snapshots(Path(game_name): Path<String>) -> impl IntoResponse {
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

async fn create_snapshot(
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

async fn restore_snapshot(
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

async fn delete_snapshot(
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

    match request.game.delete_snapshot(&request.date).await {
        Ok(_) => Json(ApiResponse::<()>::success(())).into_response(),
        Err(e) => Json(ApiResponse::<()>::error(e.to_string())).into_response(),
    }
}

async fn add_game(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AddGameRequest>,
) -> impl IntoResponse {
    if let Err(err) = verify_api_key(headers, State(state.clone())).await {
        return err.into_response();
    }

    info!(
        target: "rgsm::http_server",
        "Adding game: {:?}", request.game
    );

    match crate::backup::create_game_backup(&request.game).await {
        Ok(_) => Json(ApiResponse::<()>::success(())).into_response(),
        Err(e) => Json(ApiResponse::<()>::error(e.to_string())).into_response(),
    }
}

// Server setup
pub fn create_router(app_handle: AppHandle) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app_state = AppState {
        app_handle: Arc::new(app_handle),
    };

    Router::new()
        .route("/api/games", get(get_games))
        .route("/api/games/:game_name/snapshots", get(get_game_snapshots))
        .route("/api/snapshots/create", post(create_snapshot))
        .route("/api/snapshots/restore", post(restore_snapshot))
        .route("/api/snapshots/delete", post(delete_snapshot))
        .route("/api/games/add", post(add_game))
        .layer(cors)
        .with_state(app_state)
}

pub async fn start_server(app_handle: AppHandle) -> Result<(), anyhow::Error> {
    let config = get_config()?;
    let http_settings = &config.settings.http_server;

    if !http_settings.enabled {
        info!(target: "rgsm::http_server", "HTTP server is disabled");
        return Ok(());
    }

    let addr = format!("{}:{}", http_settings.host, http_settings.port);
    let socket_addr: SocketAddr = addr.parse()?;

    info!(target: "rgsm::http_server", "Starting HTTP server on {}", addr);

    let app = create_router(app_handle);

    tokio::spawn(async move {
        if let Err(e) = axum::serve(
            tokio::net::TcpListener::bind(socket_addr).await.unwrap(),
            app,
        )
        .await
        {
            error!(target: "rgsm::http_server", "Server error: {}", e);
        }
    });

    Ok(())
}