use log::warn;

use super::CloudSyncSessionConfig;
use super::sync_state::PendingAction;

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
