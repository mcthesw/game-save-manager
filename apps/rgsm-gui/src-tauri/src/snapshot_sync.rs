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
            let before = ConfigFingerprint::capture();
            let synced = rgsm_core::services::run_v2_snapshot_sync_once(&cancellation).await;
            before.reload_if_changed(app).await;
            synced.map_err(|error| error.to_string())?;
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
            let before = ConfigFingerprint::capture();
            if let Err(error) = rgsm_core::services::resume_v2_snapshot_sync(&cancellation).await {
                warn!("Snapshot download recovery failed: {error}");
            }
            before.reload_if_changed(app).await;
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

/// Reconciliation rewrites game definitions and removes deleted games straight
/// in the configuration store, bypassing the hook pipeline. Timers, the process
/// monitor, hotkeys and the tray keep their own copies of those games, so they
/// must be refreshed whenever the stored configuration actually changed, even
/// if reconciliation failed half-way.
struct ConfigFingerprint(Option<String>);

impl ConfigFingerprint {
    fn capture() -> Self {
        Self(
            rgsm_core::config::get_config()
                .ok()
                .and_then(|config| serde_json::to_string(&config).ok()),
        )
    }

    async fn reload_if_changed(&self, app: &AppHandle) {
        let after = Self::capture();
        if self.0 == after.0 {
            return;
        }
        match rgsm_core::config::get_config() {
            Ok(config) => {
                crate::hooks::rebuild_pipeline_and_fire_config_saved(
                    app,
                    config,
                    rgsm_core::hooks::HookSource::CloudSync,
                )
                .await;
            }
            Err(error) => {
                warn!("Failed to reload runtime state after cloud reconciliation: {error}")
            }
        }
    }
}

async fn run_reconciliation(app: &AppHandle, cancellation: &CancellationToken) {
    let before = ConfigFingerprint::capture();
    reconcile_once(app, cancellation).await;
    before.reload_if_changed(app).await;
}

async fn reconcile_once(app: &AppHandle, cancellation: &CancellationToken) {
    let state = app.state::<CloudOperationState>();
    let result = rgsm_core::services::run_v2_snapshot_sync_once(cancellation).await;
    crate::http::emit(app, "cloud-metadata-changed", &result.is_ok());
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
