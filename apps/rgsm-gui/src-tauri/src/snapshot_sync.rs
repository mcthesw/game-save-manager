use std::sync::Arc;
use std::time::Duration;

use log::{info, warn};
use tauri::{AppHandle, Manager};
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

pub fn setup(app: AppHandle, state: SnapshotSyncRuntimeState) {
    tauri::async_runtime::spawn(run(app, state));
}

async fn run(app: AppHandle, state: SnapshotSyncRuntimeState) {
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
        run_live_save_apply(&app).await;
        last_run = Instant::now();
    }
}

async fn run_live_save_apply(app: &AppHandle) {
    let targets = match rgsm_core::services::v2_live_save_sync_targets() {
        Ok(targets) => targets,
        Err(error) => {
            warn!(
                target: "rgsm::cloud::v2_live_save_sync",
                "Failed to read Live Save Sync targets: {error}"
            );
            return;
        }
    };
    for target in targets {
        let processes = match crate::process_util::running_process_names() {
            Ok(processes) => processes,
            Err(error) => {
                warn!(
                    target: "rgsm::cloud::v2_live_save_sync",
                    "Skipping Live Save Apply because process detection failed: {error}"
                );
                return;
            }
        };
        if crate::process_util::process_is_running(&processes, &target.process_name) {
            info!(
                target: "rgsm::cloud::v2_live_save_sync",
                "Skipping Live Save Apply for {} while its process is running",
                target.game_id
            );
            continue;
        }
        let plan = match rgsm_core::services::review_v2_live_save_apply(&target.game_id).await {
            Ok(Some(plan)) => plan,
            Ok(None) => continue,
            Err(error) => {
                warn!(
                    target: "rgsm::cloud::v2_live_save_sync",
                    "Failed to review Live Save progress for {}: {error}",
                    target.game_id
                );
                continue;
            }
        };
        let services = rgsm_core::services::ServiceContext::new(
            app.state::<crate::hooks::HookPipelineState>().snapshot(),
        );
        match services
            .accept_v2_remote_progress(
                &plan.game_id,
                plan.manifest_revision,
                plan.expected_local_snapshot_id.as_deref(),
                &plan.selected_snapshot_id,
            )
            .await
        {
            Ok(_) => info!(
                target: "rgsm::cloud::v2_live_save_sync",
                "Applied remote progress for {}",
                plan.game_id
            ),
            Err(error) => warn!(
                target: "rgsm::cloud::v2_live_save_sync",
                "Automatic Live Save Apply failed for {}: {error}",
                plan.game_id
            ),
        }
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
