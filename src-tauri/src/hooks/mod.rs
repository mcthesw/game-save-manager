mod pipeline;

pub mod checksum_hook;
pub mod cloud_sync_hook;
pub mod notification_hook;
pub mod pre_restore_backup_hook;

pub use pipeline::*;

use std::sync::{Arc, RwLock};

use tauri::AppHandle;

use crate::cloud_sync::{Backend, CloudSyncTaskManager};
use crate::config::Config;

pub struct HookPipelineState {
    inner: RwLock<Arc<HookPipeline>>,
}

impl HookPipelineState {
    pub fn new(pipeline: HookPipeline) -> Self {
        Self {
            inner: RwLock::new(Arc::new(pipeline)),
        }
    }

    pub fn snapshot(&self) -> Arc<HookPipeline> {
        self.inner
            .read()
            .expect("hook pipeline lock poisoned")
            .clone()
    }

    pub fn replace(&self, pipeline: HookPipeline) {
        *self.inner.write().expect("hook pipeline lock poisoned") = Arc::new(pipeline);
    }
}

pub fn build_builtin_pipeline(
    app: &AppHandle,
    task_manager: Arc<CloudSyncTaskManager>,
    config: &Config,
) -> HookPipeline {
    let mut hooks: Vec<Box<dyn SnapshotHook>> = Vec::new();

    if config.settings.extra_backup_when_apply {
        hooks.push(Box::new(pre_restore_backup_hook::PreRestoreBackupHook));
    }
    if config.settings.compute_archive_hash {
        hooks.push(Box::new(checksum_hook::ArchiveHashHook));
    }
    if config.settings.verify_archive_before_apply {
        hooks.push(Box::new(checksum_hook::ArchiveVerifyHook));
    }
    if !matches!(config.settings.cloud_settings.backend, Backend::Disabled) {
        hooks.push(Box::new(cloud_sync_hook::CloudSyncEnqueueHook::new(
            task_manager,
        )));
    }
    hooks.push(Box::new(notification_hook::NotificationHook::new(
        app.clone(),
    )));

    HookPipeline::new(hooks)
}
