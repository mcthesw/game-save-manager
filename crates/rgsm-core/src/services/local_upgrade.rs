use crate::{
    backup::local_upgrade::{LocalUpgrade, LocalUpgradeView, UpgradeRetry},
    config::{get_config, resolve_backup_path},
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
    let upgrade = LocalUpgrade::new(
        resolve_backup_path(&config.backup_path),
        get_current_device_id().clone(),
        config.games.clone(),
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
