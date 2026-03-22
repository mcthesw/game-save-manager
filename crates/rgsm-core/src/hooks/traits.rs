use async_trait::async_trait;

use super::contexts::{
    BeforeRestoreCtx, ConfigSavedCtx, GameAddedCtx, GameDeletedCtx, GameUpdatedCtx,
    MetadataChangedCtx, SnapshotAppliedCtx, SnapshotCreatedCtx, SnapshotDeletedCtx,
    SyncCompletedCtx, SyncConflictCtx,
};
use crate::cloud_sync::CloudSyncJob;
use crate::preclude::BackupError;

pub type HookResult<T> = anyhow::Result<T>;

#[async_trait]
pub trait LifecycleHook: Send + Sync {
    fn name(&self) -> &str;

    fn priority(&self) -> u32 {
        100
    }

    async fn on_snapshot_created(&self, _ctx: &mut SnapshotCreatedCtx) -> HookResult<()> {
        Ok(())
    }
    async fn on_snapshot_deleted(&self, _ctx: &SnapshotDeletedCtx) -> HookResult<()> {
        Ok(())
    }
    async fn on_before_restore(&self, _ctx: &BeforeRestoreCtx) -> Result<(), BackupError> {
        Ok(())
    }
    async fn on_snapshot_applied(&self, _ctx: &SnapshotAppliedCtx) -> HookResult<()> {
        Ok(())
    }
    async fn on_metadata_changed(&self, _ctx: &MetadataChangedCtx) -> HookResult<()> {
        Ok(())
    }
    async fn on_game_added(&self, _ctx: &GameAddedCtx) -> HookResult<()> {
        Ok(())
    }
    async fn on_game_updated(&self, _ctx: &GameUpdatedCtx) -> HookResult<()> {
        Ok(())
    }
    async fn on_game_deleted(&self, _ctx: &GameDeletedCtx) -> HookResult<()> {
        Ok(())
    }
    async fn on_config_saved(&self, _ctx: &ConfigSavedCtx) -> HookResult<()> {
        Ok(())
    }
    async fn on_sync_completed(&self, _ctx: &SyncCompletedCtx) -> HookResult<()> {
        Ok(())
    }
    async fn on_sync_conflict(&self, _ctx: &SyncConflictCtx) -> HookResult<()> {
        Ok(())
    }
}

pub use LifecycleHook as SnapshotHook;

#[async_trait]
pub trait SyncJobQueue: Send + Sync {
    async fn enqueue(&self, job: CloudSyncJob);
}

pub trait SchedulerSync: Send + Sync {
    fn sync_from_config(&self);
}
