use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type, utoipa::ToSchema)]
pub struct CloudArchiveDeletionView {
    pub snapshot_id: String,
    pub description: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type, utoipa::ToSchema)]
pub struct MaterializationPreview {
    pub snapshot_count: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type, utoipa::ToSchema)]
pub struct MaterializationOutcome {
    pub downloaded: usize,
    pub remaining: usize,
}
