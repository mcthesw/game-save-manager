use std::sync::{Arc, Mutex};

use rgsm_core::backup::{RestoreNotificationLevel, RestoreNotifier};
use rgsm_core::cloud_sync::{
    CloudSyncError, CloudSyncStatus, CloudSyncTaskManager, SyncEventEmitter,
};
use rgsm_core::config::Config;
use rgsm_core::hooks::{
    ArchiveHashHook, ArchiveVerifyHook, HookPipeline, LifecycleHook, PreRestoreBackupHook,
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
        if let Ok(mut log) = self.log.lock()
            && let Some(description) = &status.current_description
        {
            log.info(format!(
                "cloud sync: {description} ({} active)",
                status.active_jobs
            ));
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
    _settings: &TuiSettings,
    _cloud_sync_manager: Arc<CloudSyncTaskManager>,
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
    Arc::new(HookPipeline::new(hooks))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use rgsm_core::cloud_sync::{Backend, CloudSyncTaskManager};
    use rgsm_core::hooks::{ConfigSavedCtx, HookSource};

    use super::*;

    struct RecordingEmitter {
        emissions: Arc<AtomicUsize>,
    }

    impl SyncEventEmitter for RecordingEmitter {
        fn emit_status(&self, _status: &CloudSyncStatus) {
            self.emissions.fetch_add(1, Ordering::Relaxed);
        }

        fn emit_error(&self, _error: &CloudSyncError) {}
    }

    #[tokio::test]
    async fn legacy_tui_pipeline_does_not_enqueue_automatic_writes() {
        let emissions = Arc::new(AtomicUsize::new(0));
        let manager = CloudSyncTaskManager::new(Arc::new(RecordingEmitter {
            emissions: Arc::clone(&emissions),
        }));
        let mut config = Config::default();
        config.settings.cloud_settings.backend = Backend::WebDAV {
            endpoint: "https://example.invalid/dav".into(),
            username: "user".into(),
            password: "pass".into(),
        };
        let settings = TuiSettings {
            auto_enqueue_cloud_on_change: true,
            ..TuiSettings::default()
        };
        let pipeline = build_pipeline(&config, &settings, manager);

        pipeline
            .fire_config_saved(&ConfigSavedCtx {
                config,
                source: HookSource::UserManual,
            })
            .await;

        assert_eq!(emissions.load(Ordering::Relaxed), 0);
    }
}
