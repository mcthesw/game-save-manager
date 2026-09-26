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

/// Explicit refresh waits for the same cancellable reconciliation as the worker.
pub async fn refresh(
    app: &AppHandle,
) -> Result<rgsm_core::cloud_sync::v2::CloudArchiveLibraryView, String> {
    let state = app
        .state::<crate::cloud_operation::CloudOperationState>()
        .inner()
        .clone();
    state
        .run_sync(|cancellation| async move {
            rgsm_core::services::run_v2_snapshot_sync_once(&cancellation)
                .await
                .map_err(|error| error.to_string())?;
            crate::remote_progress::refresh(
                app,
                &rgsm_core::services::ServiceContext::new(
                    app.state::<crate::hooks::HookPipelineState>().snapshot(),
                ),
            )
            .await
            .map_err(|error| error.to_string())
        })
        .await
        .unwrap_or_else(
            || Err(rgsm_core::services::SnapshotSyncServiceError::Cancelled.to_string()),
        )
}

async fn run(app: AppHandle, state: CloudOperationState) {
    let operations = app
        .state::<crate::app_operations::AppOperations>()
        .inner()
        .clone();
    let app = &app;
    let operations = &operations;
    state
        .run_sync(|cancellation| async move {
            let Some(_operation) = operations.begin() else {
                return;
            };
            if let Err(error) = rgsm_core::services::resume_v2_snapshot_sync(&cancellation).await {
                warn!("Snapshot download recovery failed: {error}");
            }
            run_reconciliation(app, &cancellation).await;
        })
        .await;

    let mut last_run = Instant::now();
    let mut control_tick = tokio::time::interval(CONTROL_POLL_INTERVAL);
    control_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    control_tick.tick().await;
    loop {
        let requested = tokio::select! {
            _ = state.requested() => true,
            _ = control_tick.tick() => false,
        };
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
        if !requested && last_run.elapsed() < poll_interval {
            continue;
        }
        state
            .run_sync(|cancellation| async move {
                let Some(_operation) = operations.begin() else {
                    return;
                };
                run_reconciliation(app, &cancellation).await;
            })
            .await;
        last_run = Instant::now();
    }
}

async fn run_reconciliation(app: &AppHandle, cancellation: &CancellationToken) {
    let state = app.state::<CloudOperationState>();
    let result = rgsm_core::services::run_v2_snapshot_sync_once(cancellation).await;
    state.report_background_result(app, result.as_ref().err().map(|error| error.to_string()));
    match result {
        Ok(outcome) if outcome != Default::default() => info!(
            target: "rgsm::cloud::v2_snapshot_sync",
            "V2 Snapshot Sync completed: {} published, {} uploaded",
            outcome.published,
            outcome.uploaded
        ),
        Ok(_) => {}
        Err(error) => {
            warn!(target: "rgsm::cloud::v2_snapshot_sync", "V2 Snapshot Sync reconciliation failed: {error}");
            return;
        }
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
