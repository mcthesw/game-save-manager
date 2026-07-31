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

use log::{info, warn};
use tauri::{AppHandle, Manager};

use rgsm_core::cloud_sync::Backend;
use rgsm_core::config::{CloudNamespaceGeneration, Config, cloud_namespace_generation};

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

pub fn build_builtin_pipeline(app: &AppHandle, config: &Config) -> HookPipeline {
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
        match cloud_namespace_generation() {
            Ok(CloudNamespaceGeneration::LegacyV1) => info!(
                target: "rgsm::hooks::cloud_sync",
                "Automatic legacy cloud writes are paused until Cloud Library activation"
            ),
            Ok(CloudNamespaceGeneration::V2) => {
                let state = app.state::<crate::snapshot_sync::SnapshotSyncRuntimeState>();
                match rgsm_core::services::build_v2_snapshot_sync_hook(state.operation_lock()) {
                    Ok(Some(hook)) => hooks.push(Box::new(hook)),
                    Ok(None) => {}
                    Err(error) => warn!(
                        target: "rgsm::hooks::v2_snapshot_sync",
                        "Failed to build V2 Snapshot Sync hook: {error}"
                    ),
                }
            }
            Err(error) => warn!(
                target: "rgsm::hooks::v2_snapshot_sync",
                "Failed to determine Cloud Library generation: {error}"
            ),
        }
    }
    hooks.push(Box::new(notification_hook::NotificationHook::new(
        app.clone(),
    )));

    HookPipeline::new(hooks)
}

pub fn rebuild_pipeline(app: &AppHandle, config: &Config) -> Arc<HookPipeline> {
    let pipeline_state = app.state::<HookPipelineState>();
    pipeline_state.replace(build_builtin_pipeline(app, config))
}
