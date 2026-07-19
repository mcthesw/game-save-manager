use std::sync::Arc;
use std::time::Duration;

use log::{info, warn};
use tokio::sync::Mutex;
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

const CONTROL_POLL_INTERVAL: Duration = Duration::from_secs(15);

#[derive(Clone, Default)]
pub struct SnapshotSyncRuntimeState {
    operation_lock: Arc<Mutex<()>>,
}

impl SnapshotSyncRuntimeState {
    pub fn operation_lock(&self) -> Arc<Mutex<()>> {
        Arc::clone(&self.operation_lock)
    }
}

pub fn setup(state: SnapshotSyncRuntimeState) {
    tauri::async_runtime::spawn(run(state));
}

async fn run(state: SnapshotSyncRuntimeState) {
    let cancellation = CancellationToken::new();
    {
        let _guard = state.operation_lock.lock().await;
        match rgsm_core::services::resume_v2_snapshot_sync(&cancellation).await {
            Ok(downloaded) if downloaded > 0 => info!(
                target: "rgsm::cloud::v2_snapshot_sync",
                "Resumed {downloaded} pending Snapshot downloads at startup"
            ),
            Ok(_) => {}
            Err(error) => warn!(
                target: "rgsm::cloud::v2_snapshot_sync",
                "V2 Snapshot download recovery failed: {error}"
            ),
        }
        run_reconciliation(&cancellation).await;
    }

    let mut last_run = Instant::now();
    let mut control_tick = tokio::time::interval(CONTROL_POLL_INTERVAL);
    control_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    control_tick.tick().await;
    loop {
        control_tick.tick().await;
        let poll_minutes = match rgsm_core::services::v2_snapshot_sync_poll_minutes() {
            Ok(Some(minutes)) => minutes,
            Ok(None) => {
                last_run = Instant::now();
                continue;
            }
            Err(error) => {
                warn!(
                    target: "rgsm::cloud::v2_snapshot_sync",
                    "Failed to read V2 Snapshot Sync polling policy: {error}"
                );
                continue;
            }
        };
        let poll_interval = Duration::from_secs(poll_minutes.saturating_mul(60));
        if last_run.elapsed() < poll_interval {
            continue;
        }
        let _guard = state.operation_lock.lock().await;
        run_reconciliation(&cancellation).await;
        last_run = Instant::now();
    }
}

async fn run_reconciliation(cancellation: &CancellationToken) {
    match rgsm_core::services::run_v2_snapshot_sync_once(cancellation).await {
        Ok(outcome) if outcome != Default::default() => info!(
            target: "rgsm::cloud::v2_snapshot_sync",
            "V2 Snapshot Sync completed: {} published, {} uploaded, {} downloaded",
            outcome.published,
            outcome.uploaded,
            outcome.downloaded
        ),
        Ok(_) => {}
        Err(error) => warn!(
            target: "rgsm::cloud::v2_snapshot_sync",
            "V2 Snapshot Sync reconciliation failed: {error}"
        ),
    }
}
