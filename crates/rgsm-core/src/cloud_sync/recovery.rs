use super::{CloudSyncSessionConfig, ConflictResolution};
use crate::backup::Game;
use crate::config::Config;
use crate::preclude::BackendError;
use opendal::Operator;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolutionOutcome {
    KeptLocal,
    AcceptedRemote,
}

pub async fn resolve_game_conflict(
    _session: &CloudSyncSessionConfig,
    _op: &Operator,
    _game: &Game,
    _resolution: ConflictResolution,
) -> Result<ConflictResolutionOutcome, BackendError> {
    Err(BackendError::LegacyCloudOperationUnavailable)
}

pub async fn sync_config(
    _session: &CloudSyncSessionConfig,
    _op: &Operator,
    _config: &Config,
) -> Result<(), BackendError> {
    Err(BackendError::LegacyCloudOperationUnavailable)
}
