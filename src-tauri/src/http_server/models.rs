use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

// Application state
#[derive(Clone)]
pub struct AppState {
    pub app_handle: Arc<AppHandle>,
}

// Response types
#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(error: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
        }
    }
}

// Request types
#[derive(Deserialize)]
pub struct CreateSnapshotRequest {
    pub game: crate::backup::Game,
    pub description: String,
}

#[derive(Deserialize)]
pub struct RestoreSnapshotRequest {
    pub game: crate::backup::Game,
    pub date: String,
}

#[derive(Deserialize)]
pub struct DeleteSnapshotRequest {
    pub game: crate::backup::Game,
    pub date: String,
}

#[derive(Deserialize)]
pub struct AddGameRequest {
    pub game: crate::backup::Game,
}