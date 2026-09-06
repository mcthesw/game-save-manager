use std::time::Duration;

use log::{info, warn};
use tauri::{AppHandle, Manager};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use crate::cloud_operation::CloudOperationState;

const CONTROL_POLL_INTERVAL: Duration = Duration::from_secs(15);

pub fn setup(app: AppHandle, state: CloudOperationState) {
    tauri::async_runtime::spawn(run(app, state));
}

async fn run(app: AppHandle, state: CloudOperationState) {
    let cancellation = CancellationToken::new();
    state
        .run(async {
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
            run_reconciliation(&app, &cancellation).await;
        })
        .await;

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
        state
            .run(async {
                run_reconciliation(&app, &cancellation).await;
            })
            .await;
        last_run = Instant::now();
    }
}

async fn run_reconciliation(app: &AppHandle, cancellation: &CancellationToken) {
    match rgsm_core::services::run_v2_snapshot_sync_once(cancellation).await {
        Ok(outcome) if outcome != Default::default() => info!(
            target: "rgsm::cloud::v2_snapshot_sync",
            "V2 Snapshot Sync completed: {} published, {} uploaded",
            outcome.published,
            outcome.uploaded
        ),
        Ok(_) => {}
        Err(error) => warn!(
            target: "rgsm::cloud::v2_snapshot_sync",
            "V2 Snapshot Sync reconciliation failed: {error}"
        ),
    }
    let service = rgsm_core::services::ServiceContext::new(
        app.state::<crate::hooks::HookPipelineState>().snapshot(),
    );
    if rgsm_core::services::v2_snapshot_sync_poll_minutes().is_ok_and(|minutes| minutes.is_some())
        && let Err(error) = crate::remote_progress::refresh(app, &service).await
    {
        warn!("Remote progress refresh failed: {error}");
    }
}
