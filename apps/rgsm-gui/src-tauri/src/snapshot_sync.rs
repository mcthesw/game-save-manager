use std::time::Duration;

use log::{info, warn};
use tauri::{AppHandle, Manager};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use crate::cloud_operation::CloudOperationState;

const CONTROL_POLL_INTERVAL: Duration = Duration::from_secs(15);
const MAX_FAILURE_BACKOFF: Duration = Duration::from_secs(30 * 60);

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
            rgsm_core::cloud_sync::v2::with_read_round(async {
                let before = ConfigFingerprint::capture();
                let synced = rgsm_core::services::run_v2_snapshot_sync_once(&cancellation).await;
                before.reload_if_changed(app).await;
                if let Err(error) = &synced {
                    crate::cloud_library_cache::record_failure(error);
                }
                synced.map_err(|error| rgsm_core::services::cloud_error_message(&error))?;
                let view = crate::remote_progress::refresh(
                    app,
                    &rgsm_core::services::ServiceContext::new(
                        app.state::<crate::hooks::HookPipelineState>().snapshot(),
                    ),
                )
                .await
                .map_err(|error| rgsm_core::services::cloud_error_message(&error))?;
                crate::cloud_library_cache::store(app).await?;
                Ok(view)
            })
            .await
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
            // Recovery and the first round share one set of cloud reads.
            rgsm_core::cloud_sync::v2::with_read_round(async {
                let before = ConfigFingerprint::capture();
                if let Err(error) =
                    rgsm_core::services::resume_v2_snapshot_sync(&cancellation).await
                {
                    warn!("Snapshot download recovery failed: {error}");
                }
                before.reload_if_changed(app).await;
                run_reconciliation(app, &cancellation, false).await
            })
            .await;
        })
        .await;

    let mut last_run = Instant::now();
    let mut pending = false;
    let mut failures = 0u32;
    let mut control_tick = tokio::time::interval(CONTROL_POLL_INTERVAL);
    control_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    control_tick.tick().await;
    loop {
        tokio::select! {
            _ = state.requested() => pending = true,
            _ = control_tick.tick() => {}
        }
        let poll_minutes = match rgsm_core::services::v2_snapshot_sync_poll_minutes() {
            Ok(Some(minutes)) => minutes,
            Ok(None) => {
                last_run = Instant::now();
                pending = false;
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
        let poll_interval = failure_backoff(
            Duration::from_secs(poll_minutes.saturating_mul(60)),
            failures,
        );
        if !should_reconcile(
            pending,
            last_run.elapsed(),
            poll_interval,
            rgsm_core::cloud_sync::rate_limit::rate_limit_remaining(),
        ) {
            continue;
        }
        let requested = pending;
        let succeeded = state
            .run_sync(|cancellation| async move {
                let Some(_operation) = operations.begin() else {
                    return true;
                };
                run_reconciliation(app, &cancellation, !requested).await
            })
            .await
            .unwrap_or(true);
        failures = if succeeded {
            0
        } else {
            failures.saturating_add(1)
        };
        last_run = Instant::now();
        pending = false;
    }
}

/// Repeated failures slow the timer down (poll, 2x, 4x, ... up to 30 minutes)
/// so an unreachable or throttling provider is not hammered.
fn failure_backoff(poll_interval: Duration, failures: u32) -> Duration {
    let backed_off = poll_interval.saturating_mul(1u32.checked_shl(failures).unwrap_or(u32::MAX));
    backed_off.min(MAX_FAILURE_BACKOFF.max(poll_interval))
}

/// While the provider is throttling, a request that arrived meanwhile stays
/// pending and runs once the cooldown ends instead of extending the block.
fn should_reconcile(
    pending: bool,
    since_last_run: Duration,
    poll_interval: Duration,
    rate_limit_remaining: Option<Duration>,
) -> bool {
    rate_limit_remaining.is_none() && (pending || since_last_run >= poll_interval)
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

/// `true` when the round succeeded or was skipped as unnecessary.
async fn run_reconciliation(app: &AppHandle, cancellation: &CancellationToken, poll: bool) -> bool {
    let before = ConfigFingerprint::capture();
    let succeeded =
        rgsm_core::cloud_sync::v2::with_read_round(reconcile_once(app, cancellation, poll)).await;
    before.reload_if_changed(app).await;
    succeeded
}

async fn reconcile_once(app: &AppHandle, cancellation: &CancellationToken, poll: bool) -> bool {
    let result = if poll {
        rgsm_core::services::run_v2_snapshot_sync_if_changed(cancellation).await
    } else {
        rgsm_core::services::run_v2_snapshot_sync_once(cancellation)
            .await
            .map(Some)
    };
    let outcome = match result {
        Ok(Some(outcome)) => outcome,
        Ok(None) => {
            crate::cloud_library_cache::mark_checked();
            return true;
        }
        Err(error) => {
            crate::cloud_library_cache::record_failure(&error);
            crate::http::emit(app, "cloud-metadata-changed", &false);
            warn!(target: "rgsm::cloud::v2_snapshot_sync", "V2 Snapshot Sync reconciliation failed: {error}");
            return false;
        }
    };
    crate::http::emit(app, "cloud-metadata-changed", &true);
    if outcome != Default::default() {
        info!(
            target: "rgsm::cloud::v2_snapshot_sync",
            "V2 Snapshot Sync completed: {} published, {} uploaded",
            outcome.published,
            outcome.uploaded
        );
    }
    let service = rgsm_core::services::ServiceContext::new(
        app.state::<crate::hooks::HookPipelineState>().snapshot(),
    );
    if rgsm_core::services::v2_snapshot_sync_poll_minutes().is_ok_and(|minutes| minutes.is_some())
        && let Err(error) = crate::remote_progress::refresh(app, &service).await
    {
        warn!("Remote progress refresh failed: {error}");
    }
    if let Err(error) = crate::cloud_library_cache::store(app).await {
        warn!("Cloud library state refresh failed: {error}");
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLL: Duration = Duration::from_secs(300);

    #[test]
    fn failures_back_off_exponentially_up_to_thirty_minutes() {
        let minutes = |failures| failure_backoff(POLL, failures).as_secs() / 60;
        assert_eq!([0, 1, 2, 3, 9, 40].map(minutes), [5, 10, 20, 30, 30, 30]);
        assert_eq!(
            failure_backoff(Duration::from_secs(3600), 3),
            Duration::from_secs(3600)
        );
    }

    #[test]
    fn automatic_runs_wait_out_a_provider_rate_limit() {
        let cooldown = Some(Duration::from_secs(60));
        assert!(!should_reconcile(true, POLL * 2, POLL, cooldown));
        assert!(!should_reconcile(false, POLL * 2, POLL, cooldown));
    }

    #[test]
    fn requests_and_elapsed_polls_run_when_not_limited() {
        assert!(should_reconcile(true, Duration::ZERO, POLL, None));
        assert!(should_reconcile(false, POLL, POLL, None));
        assert!(!should_reconcile(false, Duration::ZERO, POLL, None));
    }
}
