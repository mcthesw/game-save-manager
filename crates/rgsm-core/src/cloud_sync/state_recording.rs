use log::warn;

use super::CloudSyncSessionConfig;
use super::sync_state::{
    GameSyncState, PendingAction, SyncResult, build_game_sync_state, update_config_sync_state,
    update_game_sync_state, with_sync_state,
};
use crate::backup::GameSnapshots;

pub(super) fn current_device_head(info: &GameSnapshots) -> Option<String> {
    info.current_device_head_cloned()
}

pub(super) fn record_config_state(
    session: &CloudSyncSessionConfig,
    result: SyncResult,
    pending: PendingAction,
) {
    let state = build_game_sync_state(None, None, result, pending);
    if let Err(err) = with_sync_state(|sync_state| {
        update_config_sync_state(sync_state, session, state);
    }) {
        warn!("Failed to record config sync state: {err}");
    }
}

pub(super) fn record_game_state(
    session: &CloudSyncSessionConfig,
    game_name: &str,
    local_head: Option<String>,
    remote_head: Option<String>,
    result: SyncResult,
    pending: PendingAction,
) {
    let state = build_game_sync_state(local_head, remote_head, result, pending);
    let game_name = game_name.to_string();
    if let Err(err) = with_sync_state(|sync_state| {
        update_game_sync_state(sync_state, session, &game_name, state);
    }) {
        warn!("Failed to record sync state for {game_name}: {err}");
    }
}

pub(super) fn merged_error_state(
    previous: Option<&GameSyncState>,
    local_head: Option<String>,
    remote_head: Option<String>,
    message: String,
    pending: PendingAction,
) -> (Option<String>, Option<String>, SyncResult, PendingAction) {
    let local_head =
        local_head.or_else(|| previous.and_then(|state| state.last_known_local_head.clone()));
    let remote_head =
        remote_head.or_else(|| previous.and_then(|state| state.last_known_remote_head.clone()));
    (local_head, remote_head, SyncResult::Error(message), pending)
}

pub(super) fn log_game_sync_failure(
    session: &CloudSyncSessionConfig,
    game_name: &str,
    operation: &str,
    pending: PendingAction,
    error_message: &str,
) {
    warn!(
        target: "rgsm::cloud::diagnostics",
        "Game cloud sync failed: game={game_name}, operation={operation}, backend_fingerprint={}, pending_action={pending:?}, error={error_message}",
        session.fingerprint()
    );
}

pub(super) fn log_config_sync_failure(
    session: &CloudSyncSessionConfig,
    operation: &str,
    error_message: &str,
) {
    warn!(
        target: "rgsm::cloud::diagnostics",
        "Config cloud sync failed: operation={operation}, backend_fingerprint={}, error={error_message}",
        session.fingerprint()
    );
}
