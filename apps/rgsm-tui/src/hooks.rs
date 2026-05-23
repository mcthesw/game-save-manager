use std::sync::{Arc, Mutex};

use rgsm_core::backup::{RestoreNotificationLevel, RestoreNotifier};
use rgsm_core::cloud_sync::{
    Backend, CloudSyncError, CloudSyncStatus, CloudSyncTaskManager, SyncEventEmitter,
};
use rgsm_core::config::Config;
use rgsm_core::hooks::{
    ArchiveHashHook, ArchiveVerifyHook, CloudSyncEnqueueHook, HookPipeline, LifecycleHook,
    PreRestoreBackupHook, SyncJobQueue,
};

use crate::logging::SessionLog;
use crate::tui_settings::TuiSettings;

pub struct TuiRestoreNotifier {
    log: Arc<Mutex<SessionLog>>,
}

impl TuiRestoreNotifier {
    pub fn new(log: Arc<Mutex<SessionLog>>) -> Self {
        Self { log }
    }
}

impl RestoreNotifier for TuiRestoreNotifier {
    fn notify(&self, level: RestoreNotificationLevel, title: &str, msg: &str) {
        let line = format!("{title}: {msg}");
        let Ok(mut log) = self.log.lock() else {
            return;
        };
        match level {
            RestoreNotificationLevel::Info => log.info(line),
            RestoreNotificationLevel::Warning => log.warn(line),
        }
    }
}

pub struct TuiSyncEmitter {
    log: Arc<Mutex<SessionLog>>,
}

impl TuiSyncEmitter {
    pub fn new(log: Arc<Mutex<SessionLog>>) -> Self {
        Self { log }
    }
}

impl SyncEventEmitter for TuiSyncEmitter {
    fn emit_status(&self, status: &CloudSyncStatus) {
        if let Ok(mut log) = self.log.lock() {
            if let Some(description) = &status.current_description {
                log.info(format!(
                    "cloud sync: {description} ({} active)",
                    status.active_jobs
                ));
            }
        }
    }

    fn emit_error(&self, error: &CloudSyncError) {
        if let Ok(mut log) = self.log.lock() {
            match &error.game_name {
                Some(game) => log.error(format!("cloud sync failed for {game}: {}", error.error)),
                None => log.error(format!("cloud sync failed: {}", error.error)),
            }
        }
    }
}

pub fn build_pipeline(
    config: &Config,
    settings: &TuiSettings,
    cloud_sync_manager: Arc<CloudSyncTaskManager>,
) -> Arc<HookPipeline> {
    let mut hooks: Vec<Box<dyn LifecycleHook>> = Vec::new();
    if config.settings.extra_backup_when_apply {
        hooks.push(Box::new(PreRestoreBackupHook));
    }
    if config.settings.compute_archive_hash {
        hooks.push(Box::new(ArchiveHashHook));
    }
    if config.settings.verify_archive_before_apply {
        hooks.push(Box::new(ArchiveVerifyHook));
    }
    if settings.auto_enqueue_cloud_on_change
        && !matches!(config.settings.cloud_settings.backend, Backend::Disabled)
    {
        let sync_queue: Arc<dyn SyncJobQueue> = cloud_sync_manager;
        hooks.push(Box::new(CloudSyncEnqueueHook::new(sync_queue)));
    }
    Arc::new(HookPipeline::new(hooks))
}
