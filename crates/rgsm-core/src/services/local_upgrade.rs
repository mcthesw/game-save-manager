use crate::{
    backup::{
        Game,
        local_upgrade::{LocalUpgrade, LocalUpgradeView, UpgradeRetry},
    },
    cloud_sync::Backend,
    config::{
        CloudNamespaceGeneration, Config, LocalState, SharedLibrary, cloud_bootstrap_inputs,
        get_config, resolve_backup_path,
    },
    device::get_current_device_id,
    preclude::BackupError,
};

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum LocalUpgradeAction {
    Preview,
    Start,
    Step,
    Retry { request: UpgradeRetry },
    Cleanup,
}

/// Assemble local dependencies once per request. Archive conversion itself has
/// no access to global configuration, hooks, live saves or cloud storage.
pub fn local_archive_upgrade(action: LocalUpgradeAction) -> Result<LocalUpgradeView, BackupError> {
    let config = get_config()?;
    let (library, _, local_state) = cloud_bootstrap_inputs()?;
    let upgrade = LocalUpgrade::new(
        resolve_backup_path(&config.backup_path),
        get_current_device_id().clone(),
        games_outside_cloud_library(&config, &library, &local_state),
        config.settings.compression_preset,
    );
    drop(config);
    match action {
        LocalUpgradeAction::Preview => upgrade.inspect(),
        LocalUpgradeAction::Start => upgrade.start(),
        LocalUpgradeAction::Step => upgrade.step(),
        LocalUpgradeAction::Retry { request } => upgrade.retry(request),
        LocalUpgradeAction::Cleanup => upgrade.cleanup_originals(),
    }
}

/// A converted archive has different bytes, hash and extension than the one a
/// Cloud Library may already hold, so games whose history is (or may be)
/// shared keep their archives as they are. The Library keeps serving them
/// and new Snapshots use the current format.
fn games_outside_cloud_library(
    config: &Config,
    library: &SharedLibrary,
    local_state: &LocalState,
) -> Vec<Game> {
    let legacy_cloud_connected =
        !matches!(config.settings.cloud_settings.backend, Backend::Disabled);
    config
        .games
        .iter()
        .filter(|game| {
            if local_state.cloud_namespace_generation == CloudNamespaceGeneration::V2 {
                !local_state.is_cloud_library_game(library, &game.storage_key)
            } else {
                !(legacy_cloud_connected && game.cloud_sync_enabled)
            }
        })
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "local_upgrade_tests.rs"]
mod tests;
