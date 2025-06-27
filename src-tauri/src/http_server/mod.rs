mod handlers;
mod middleware;
mod models;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};
use log::{error, info};
use tauri::AppHandle;
use tower_http::cors::{Any, CorsLayer};

pub use models::AppState;
use crate::config::get_config;

// Create router with all routes
fn create_router(app_handle: AppHandle) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let state = models::AppState {
        app_handle: Arc::new(app_handle),
    };

    Router::new()
        .route("/api/games", get(handlers::get_games))
        .route("/api/games/:game_name/snapshots", get(handlers::get_game_snapshots))
        .route("/api/snapshots/create", post(handlers::create_snapshot))
        .route("/api/snapshots/restore", post(handlers::restore_snapshot))
        .route("/api/snapshots/delete", post(handlers::delete_snapshot))
        .route("/api/games/add", post(handlers::add_game))
        .layer(cors)
        .with_state(state)
}

/// Start the HTTP server
pub async fn start_server(app_handle: AppHandle) -> Result<(), anyhow::Error> {
    let config = get_config()?;
    let http_settings = config.settings.http_server;

    let addr = format!("{}:{}", http_settings.host, http_settings.port);
    let socket_addr: SocketAddr = addr.parse()?;

    info!(target: "rgsm::http_server", "Starting HTTP server on {}", addr);

    let app = create_router(app_handle);

    // 直接启动服务器，不需要额外的tokio::spawn
    if let Err(e) = axum::serve(
        tokio::net::TcpListener::bind(socket_addr).await.unwrap(),
        app,
    )
    .await
    {
        error!(target: "rgsm::http_server", "Server error: {}", e);
    }

    Ok(())
}