//! Hook pipeline composition and built-in side-effect registration.
//!
//! Core hooks (pipeline, checksum, cloud_sync, pre_restore) live in rgsm_core.
//! GUI-specific hooks (notification, quick_action_sync, scheduler_sync) live here.

pub mod notification_hook;
mod process_monitor_sync_hook;
mod quick_action_sync_hook;
mod runtime_config_sync;
mod scheduler_sync_hook;

// Re-export everything from core hooks so the rest of the GUI can `use crate::hooks::*`
pub use rgsm_core::hooks::*;

use std::sync::{Arc, RwLock};

use tauri::{AppHandle, Manager};

use rgsm_core::cloud_sync::{Backend, CloudSyncTaskManager};
use rgsm_core::config::Config;

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

    pub fn replace(&self, pipeline: HookPipeline) -> Arc<HookPipeline> {
        let pipeline = Arc::new(pipeline);
        *self.inner.write().expect("hook pipeline lock poisoned") = pipeline.clone();
        pipeline
    }
}

pub fn build_builtin_pipeline(
    app: &AppHandle,
    task_manager: Arc<CloudSyncTaskManager>,
    config: &Config,
) -> HookPipeline {
    let mut hooks: Vec<Box<dyn LifecycleHook>> = Vec::new();

    if config.settings.extra_backup_when_apply {
        hooks.push(Box::new(
            rgsm_core::hooks::pre_restore_backup_hook::PreRestoreBackupHook,
        ));
    }
    if config.settings.compute_archive_hash {
        hooks.push(Box::new(rgsm_core::hooks::checksum_hook::ArchiveHashHook));
    }
    if config.settings.verify_archive_before_apply {
        hooks.push(Box::new(rgsm_core::hooks::checksum_hook::ArchiveVerifyHook));
    }
    let scheduler_sync: Arc<dyn runtime_config_sync::ConfigRuntimeSync> =
        Arc::new(scheduler_sync_hook::TauriSchedulerSync::new(app.clone()));
    hooks.push(Box::new(scheduler_sync_hook::SchedulerSyncHook::new(
        scheduler_sync,
    )));
    let process_monitor_sync: Arc<dyn runtime_config_sync::ConfigRuntimeSync> = Arc::new(
        process_monitor_sync_hook::TauriProcessMonitorSync::new(app.clone()),
    );
    hooks.push(Box::new(
        process_monitor_sync_hook::ProcessMonitorSyncHook::new(process_monitor_sync),
    ));
    let quick_action_sync: Arc<dyn runtime_config_sync::ConfigRuntimeSync> = Arc::new(
        quick_action_sync_hook::TauriQuickActionRuntimeSync::new(app.clone()),
    );
    hooks.push(Box::new(quick_action_sync_hook::QuickActionSyncHook::new(
        quick_action_sync,
    )));
    if !matches!(config.settings.cloud_settings.backend, Backend::Disabled) {
        let sync_queue: Arc<dyn SyncJobQueue> = task_manager;
        hooks.push(Box::new(
            rgsm_core::hooks::cloud_sync_hook::CloudSyncEnqueueHook::new(sync_queue),
        ));
    }
    hooks.push(Box::new(notification_hook::NotificationHook::new(
        app.clone(),
    )));

    HookPipeline::new(hooks)
}

pub fn rebuild_pipeline(app: &AppHandle, config: &Config) -> Arc<HookPipeline> {
    let pipeline_state = app.state::<HookPipelineState>();
    let cloud_sync_manager = app.state::<Arc<CloudSyncTaskManager>>().inner().clone();
    pipeline_state.replace(build_builtin_pipeline(app, cloud_sync_manager, config))
}
