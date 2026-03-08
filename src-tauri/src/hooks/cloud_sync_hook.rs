use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use log::info;

use super::pipeline::{
    ConfigSavedCtx, GameAddedCtx, GameDeletedCtx, GameUpdatedCtx, HookSource, MetadataChangedCtx,
    SnapshotCreatedCtx, SnapshotDeletedCtx, SnapshotHook,
};
use crate::cloud_sync::{CloudSyncJob, CloudSyncTaskManager};

/// Enqueues cloud-sync jobs according to per-game `cloud_sync_enabled`.
///
/// Priority 50 — after checksum so uploaded metadata already contains
/// the verified `archive_hash`.
pub struct CloudSyncEnqueueHook {
    task_manager: Arc<CloudSyncTaskManager>,
}

impl CloudSyncEnqueueHook {
    pub fn new(task_manager: Arc<CloudSyncTaskManager>) -> Self {
        Self { task_manager }
    }

    fn should_sync(config: &crate::config::Config, game_name: &str, source: &HookSource) -> bool {
        // Never re-enqueue from CloudSync source (avoid infinite loop).
        if *source == HookSource::CloudSync {
            return false;
        }
        config
            .games
            .iter()
            .find(|g| g.name == game_name)
            .map(|g| g.cloud_sync_enabled)
            .unwrap_or(false)
    }

    fn backend(config: &crate::config::Config) -> Option<crate::cloud_sync::Backend> {
        match &config.settings.cloud_settings.backend {
            crate::cloud_sync::Backend::Disabled => None,
            backend => Some(backend.clone()),
        }
    }
}

#[async_trait]
impl SnapshotHook for CloudSyncEnqueueHook {
    fn name(&self) -> &str {
        "CloudSyncEnqueueHook"
    }

    fn priority(&self) -> u32 {
        50
    }

    async fn on_snapshot_created(&self, ctx: &mut SnapshotCreatedCtx) -> Result<()> {
        if !Self::should_sync(&ctx.config, &ctx.game.name, &ctx.source) {
            return Ok(());
        }
        let Some(backend) = Self::backend(&ctx.config) else {
            return Ok(());
        };
        info!(
            target: "rgsm::hooks::cloud_sync",
            "Enqueuing upload for new snapshot: {} / {}",
            ctx.game.name, ctx.snapshot.date
        );
        self.task_manager
            .enqueue(CloudSyncJob::UploadSnapshot {
                backend,
                game_name: ctx.snapshots.name.clone(),
                snapshots: ctx.snapshots.clone(),
                local_zip_path: ctx.local_zip_path.clone(),
                remote_zip_path: ctx.remote_zip_path.clone(),
            })
            .await;
        Ok(())
    }

    async fn on_snapshot_deleted(&self, ctx: &SnapshotDeletedCtx) -> Result<()> {
        if !Self::should_sync(&ctx.config, &ctx.game.name, &ctx.source) {
            return Ok(());
        }
        let Some(backend) = Self::backend(&ctx.config) else {
            return Ok(());
        };
        if ctx.deleted_remote_paths.is_empty() {
            // Just metadata change, upload metadata only.
            self.task_manager
                .enqueue(CloudSyncJob::UploadMetadata {
                    backend,
                    game_name: ctx.snapshots.name.clone(),
                    snapshots: ctx.snapshots.clone(),
                })
                .await;
        } else if ctx.deleted_remote_paths.len() == 1 {
            self.task_manager
                .enqueue(CloudSyncJob::DeleteSnapshotAndUploadMetadata {
                    backend,
                    game_name: ctx.snapshots.name.clone(),
                    snapshots: ctx.snapshots.clone(),
                    remote_zip_path: ctx.deleted_remote_paths[0].clone(),
                })
                .await;
        } else {
            self.task_manager
                .enqueue(CloudSyncJob::DeleteFilesAndUploadMetadata {
                    backend,
                    game_name: ctx.snapshots.name.clone(),
                    snapshots: ctx.snapshots.clone(),
                    remote_zip_paths: ctx.deleted_remote_paths.clone(),
                })
                .await;
        }
        Ok(())
    }

    async fn on_metadata_changed(&self, ctx: &MetadataChangedCtx) -> Result<()> {
        if !Self::should_sync(&ctx.config, &ctx.game.name, &ctx.source) {
            return Ok(());
        }
        let Some(backend) = Self::backend(&ctx.config) else {
            return Ok(());
        };
        self.task_manager
            .enqueue(CloudSyncJob::UploadMetadata {
                backend,
                game_name: ctx.snapshots.name.clone(),
                snapshots: ctx.snapshots.clone(),
            })
            .await;
        Ok(())
    }

    async fn on_game_added(&self, ctx: &GameAddedCtx) -> Result<()> {
        let Some(backend) = Self::backend(&ctx.config) else {
            return Ok(());
        };
        if Self::should_sync(&ctx.config, &ctx.game.name, &ctx.source) {
            self.task_manager
                .enqueue(CloudSyncJob::UploadMetadata {
                    backend: backend.clone(),
                    game_name: ctx.snapshots.name.clone(),
                    snapshots: ctx.snapshots.clone(),
                })
                .await;
        }
        self.task_manager
            .enqueue(CloudSyncJob::UploadConfig {
                backend,
                context: "game_added".to_string(),
            })
            .await;
        Ok(())
    }

    async fn on_game_updated(&self, ctx: &GameUpdatedCtx) -> Result<()> {
        if ctx.source == HookSource::CloudSync {
            return Ok(());
        }
        let Some(backend) = Self::backend(&ctx.config) else {
            return Ok(());
        };
        self.task_manager
            .enqueue(CloudSyncJob::UploadConfig {
                backend,
                context: format!("game_updated:{}", ctx.game.name),
            })
            .await;
        Ok(())
    }

    async fn on_game_deleted(&self, ctx: &GameDeletedCtx) -> Result<()> {
        if ctx.source == HookSource::CloudSync {
            return Ok(());
        }
        let Some(backend) = Self::backend(&ctx.config) else {
            return Ok(());
        };
        self.task_manager
            .enqueue(CloudSyncJob::DeleteGameAndUploadConfig {
                backend,
                game_name: ctx.game_name.clone(),
                remote_game_dir_path: ctx.remote_game_dir_path.clone(),
            })
            .await;
        Ok(())
    }

    async fn on_config_saved(&self, ctx: &ConfigSavedCtx) -> Result<()> {
        if ctx.source == HookSource::CloudSync {
            return Ok(());
        }
        let Some(backend) = Self::backend(&ctx.config) else {
            return Ok(());
        };
        self.task_manager
            .enqueue(CloudSyncJob::UploadConfig {
                backend,
                context: "config_saved".to_string(),
            })
            .await;
        Ok(())
    }
}
