use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use log::info;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::{HookSource, LifecycleHook, SnapshotCreatedCtx};
use crate::cloud_sync::v2::SnapshotSyncCoordinator;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotSyncTarget {
    pub activation_revision: u64,
    pub local_baseline: BTreeSet<String>,
    pub retention_limit: Option<u32>,
    pub upload_new_archives: bool,
    pub download_forward_target: bool,
    /// True when the game uses Multi-device Sync. The sync loop uses this to
    /// perform publication-only reconciliation before checking divergence,
    /// preventing stale suspension flags from allowing transfers.
    pub is_multi_device_sync: bool,
}

pub struct V2SnapshotSyncHook {
    coordinator: SnapshotSyncCoordinator,
    targets: BTreeMap<String, SnapshotSyncTarget>,
    operation_lock: Arc<Mutex<()>>,
}

impl V2SnapshotSyncHook {
    pub fn new(
        coordinator: SnapshotSyncCoordinator,
        targets: BTreeMap<String, SnapshotSyncTarget>,
        operation_lock: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            coordinator,
            targets,
            operation_lock,
        }
    }
}

#[async_trait]
impl LifecycleHook for V2SnapshotSyncHook {
    fn name(&self) -> &str {
        "V2SnapshotSyncHook"
    }

    fn priority(&self) -> u32 {
        50
    }

    async fn on_snapshot_created(&self, ctx: &mut SnapshotCreatedCtx) -> Result<()> {
        if ctx.source == HookSource::CloudSync {
            return Ok(());
        }
        let game_id = ctx.game.backup_dir_name();
        let Some(target) = self.targets.get(game_id.as_ref()) else {
            return Ok(());
        };
        let _guard = self.operation_lock.lock().await;

        // Multi-device Sync: publish records first, check divergence, then
        // transfer only when the fleet is not diverged. This prevents a stale
        // target from uploading a new archive after suspension.
        let allow_transfer = if target.is_multi_device_sync {
            self.coordinator
                .reconcile_game_with_policy(
                    game_id.as_ref(),
                    &ctx.snapshots,
                    target.activation_revision,
                    &target.local_baseline,
                    &CancellationToken::new(),
                    crate::cloud_sync::v2::SnapshotReconcilePolicy {
                        upload_new_archives: false,
                        download_forward_target: false,
                    },
                )
                .await?;
            !self
                .coordinator
                .check_divergence(game_id.as_ref(), &ctx.snapshots)
                .await?
        } else {
            true
        };

        let outcome = if allow_transfer {
            self.coordinator
                .reconcile_game_with_policy(
                    game_id.as_ref(),
                    &ctx.snapshots,
                    target.activation_revision,
                    &target.local_baseline,
                    &CancellationToken::new(),
                    crate::cloud_sync::v2::SnapshotReconcilePolicy {
                        upload_new_archives: target.upload_new_archives,
                        download_forward_target: false,
                    },
                )
                .await?
        } else {
            crate::cloud_sync::v2::SnapshotReconciliationOutcome::default()
        };
        let retained = if let Some(limit) = target.retention_limit {
            let retention = self
                .coordinator
                .enforce_retention(game_id.as_ref(), limit)
                .await?;
            ctx.snapshots.forget_v2_tombstones(&retention.tombstones);
            retention.deleted
        } else {
            0
        };
        info!(
            target: "rgsm::hooks::v2_snapshot_sync",
            "Reconciled {} after Snapshot creation: {} published, {} uploaded, {} downloaded, {} retained-history deletions",
            game_id,
            outcome.published,
            outcome.uploaded,
            outcome.downloaded,
            retained,
        );
        Ok(())
    }
}
