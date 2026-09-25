use super::CloudSyncSessionConfig;
use crate::preclude::BackendError;
use serde::{Deserialize, Serialize};
use specta::Type;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SyncGameOutcome {
    AlreadyInSync,
    Uploaded,
    Downloaded,
    Merged,
    Conflict,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BatchSyncItemStatus {
    Success,
    Cancelled,
    Failed(String),
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, utoipa::ToSchema)]
pub struct BatchSyncItemReport {
    pub name: String,
    pub status: BatchSyncItemStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, utoipa::ToSchema)]
pub struct BatchSyncReport {
    pub config: BatchSyncItemReport,
    pub games: Vec<BatchSyncItemReport>,
}

// Retained for callers of the old HTTP/core contract. Legacy operations never perform I/O.
pub async fn upload_all_from_session(
    _session: &CloudSyncSessionConfig,
    _token: Option<CancellationToken>,
) -> Result<BatchSyncReport, BackendError> {
    Err(BackendError::LegacyCloudOperationUnavailable)
}

pub async fn download_all_from_session(
    _session: &CloudSyncSessionConfig,
    _token: Option<CancellationToken>,
) -> Result<BatchSyncReport, BackendError> {
    Err(BackendError::LegacyCloudOperationUnavailable)
}

pub async fn sync_game(
    _session: &CloudSyncSessionConfig,
    _op: &opendal::Operator,
    _game: &crate::backup::Game,
) -> Result<SyncGameOutcome, BackendError> {
    Err(BackendError::LegacyCloudOperationUnavailable)
}
