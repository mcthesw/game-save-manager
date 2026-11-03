//! Shows Cloud transfers as a job with progress. A job created here appears
//! when the first Archive is announced, so a round that moves nothing stays
//! invisible; `attached` reports into a job the caller already owns.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use rgsm_core::cloud_sync::progress::{TransferProgress, TransferReporter, with_transfer_reporter};
use rgsm_core::cloud_sync::{CloudSyncJobStatus, CloudSyncTaskManager};
use tauri::{AppHandle, Manager};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

const EMIT_INTERVAL: Duration = Duration::from_millis(250);

type Outcome = (CloudSyncJobStatus, Option<String>);

pub enum Job {
    /// Created on the first transfer; finished with the outcome of the work.
    Lazy(String),
    /// Owned by the caller, which also finishes it.
    Attached(u64),
}

fn manager(app: &AppHandle) -> Arc<CloudSyncTaskManager> {
    Arc::clone(app.state::<Arc<CloudSyncTaskManager>>().inner())
}

/// Run `work` with its transfers reported to `job`. `outcome` classifies the
/// result of the work for a lazily created job. `work` arrives boxed because a
/// Cloud round is a very large future in unoptimised builds: holding it inline
/// would enlarge every caller, including the ones spawned from the main thread.
pub async fn track<T>(
    app: &AppHandle,
    job: Job,
    work: Pin<Box<dyn Future<Output = T> + Send + '_>>,
    outcome: impl FnOnce(&T) -> Outcome,
) -> T {
    let (sender, updates) = mpsc::unbounded_channel();
    let (finish, finished) = oneshot::channel();
    let forwarder = tauri::async_runtime::spawn(forward(manager(app), job, updates, finished));
    let reporter = TransferReporter::new(move |progress| {
        let _ = sender.send(progress);
    });
    let result = with_transfer_reporter(reporter, work).await;
    let _ = finish.send(outcome(&result));
    let _ = forwarder.await;
    result
}

async fn forward(
    manager: Arc<CloudSyncTaskManager>,
    job: Job,
    mut updates: mpsc::UnboundedReceiver<TransferProgress>,
    mut finished: oneshot::Receiver<Outcome>,
) {
    let (description, mut job_id) = match job {
        Job::Lazy(description) => (Some(description), None),
        Job::Attached(id) => (None, Some(id)),
    };
    let mut last_emit: Option<Instant> = None;
    let mut latest = None;
    let outcome = loop {
        tokio::select! {
            Some(progress) = updates.recv() => {
                latest = Some(progress);
                if last_emit.is_none_or(|at| at.elapsed() >= EMIT_INTERVAL) {
                    publish(&manager, description.as_deref(), &mut job_id, progress).await;
                    last_emit = Some(Instant::now());
                }
            }
            outcome = &mut finished => break outcome.ok(),
        }
    };
    while let Ok(progress) = updates.try_recv() {
        latest = Some(progress);
    }
    if let (Some(description), Some(progress)) = (description.as_deref(), latest) {
        publish(&manager, Some(description), &mut job_id, progress).await;
        if let (Some(id), Some((status, error))) = (job_id, outcome) {
            manager
                .finish_manual_job(id, description, status, error)
                .await;
        }
    }
}

async fn publish(
    manager: &CloudSyncTaskManager,
    description: Option<&str>,
    job_id: &mut Option<u64>,
    progress: TransferProgress,
) {
    if job_id.is_none()
        && let Some(description) = description
    {
        *job_id = Some(manager.begin_manual_job(description.to_string()).await.0);
    }
    if let Some(id) = *job_id {
        manager.report_progress(id, progress).await;
    }
}

pub fn outcome<T, E: ToString>(result: &Result<T, E>, cancelled: impl FnOnce() -> bool) -> Outcome {
    match result {
        Ok(_) => (CloudSyncJobStatus::Completed, None),
        Err(_) if cancelled() => (CloudSyncJobStatus::Cancelled, None),
        Err(error) => (CloudSyncJobStatus::Failed, Some(error.to_string())),
    }
}
