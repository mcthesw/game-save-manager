use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(test)]
use std::time::Duration;

use log::warn;
use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

/// Trait for emitting cloud sync events to the frontend.
///
/// GUI implements this using Tauri's event system; CLI might log to stdout.
pub trait SyncEventEmitter: Send + Sync {
    fn emit_status(&self, status: &CloudSyncStatus);
    fn emit_error(&self, error: &CloudSyncError);
}

const MAX_HISTORY_SIZE: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub enum CloudSyncJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct CloudSyncJobInfo {
    pub id: u64,
    pub description: String,
    pub status: CloudSyncJobStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct CloudSyncStatus {
    pub active_jobs: usize,
    pub current_description: Option<String>,
    pub jobs: Vec<CloudSyncJobInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct CloudSyncError {
    pub game_name: Option<String>,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CancelCloudSyncResult {
    Cancelled,
    NoActiveOperations,
}

#[derive(Debug)]
struct CloudSyncState {
    manual_cancel_token: CancellationToken,
    next_id: u64,
    running_jobs: Vec<CloudSyncJobInfo>,
    history: VecDeque<CloudSyncJobInfo>,
}

impl Default for CloudSyncState {
    fn default() -> Self {
        Self {
            manual_cancel_token: CancellationToken::new(),
            next_id: 1,
            running_jobs: Vec::new(),
            history: VecDeque::new(),
        }
    }
}

pub struct CloudSyncTaskManager {
    emitter: Arc<dyn SyncEventEmitter>,
    state: Mutex<CloudSyncState>,
    running_count: AtomicUsize,
    notify: Notify,
}

impl CloudSyncTaskManager {
    pub fn new(emitter: Arc<dyn SyncEventEmitter>) -> Arc<Self> {
        Arc::new(Self {
            emitter,
            state: Mutex::new(CloudSyncState::default()),
            running_count: AtomicUsize::new(0),
            notify: Notify::new(),
        })
    }

    pub async fn cancel_all(&self) -> CancelCloudSyncResult {
        let had_active = self.running_count.load(Ordering::Relaxed) > 0;
        {
            let mut state = self.state.lock().await;
            state.manual_cancel_token.cancel();
            state.manual_cancel_token = CancellationToken::new();
        }

        self.notify.notify_one();
        self.emit_full_status(None).await;
        if had_active {
            CancelCloudSyncResult::Cancelled
        } else {
            CancelCloudSyncResult::NoActiveOperations
        }
    }

    pub async fn cancel_all_and_wait(&self) -> CancelCloudSyncResult {
        let result = self.cancel_all().await;
        loop {
            let idle = self.notify.notified();
            if self.running_count.load(Ordering::Acquire) == 0 {
                return result;
            }
            idle.await;
        }
    }

    pub async fn begin_manual_job(&self, description: String) -> (u64, CancellationToken) {
        let (id, token) = {
            let mut state = self.state.lock().await;
            let id = state.next_id;
            state.next_id += 1;
            state.running_jobs.push(CloudSyncJobInfo {
                id,
                description: description.clone(),
                status: CloudSyncJobStatus::Running,
                error: None,
            });
            (id, state.manual_cancel_token.child_token())
        };
        self.running_count.fetch_add(1, Ordering::Relaxed);
        self.emit_full_status(Some(description)).await;
        (id, token)
    }

    pub async fn finish_manual_job(
        self: &Arc<Self>,
        id: u64,
        description: &str,
        status: CloudSyncJobStatus,
        error: Option<String>,
    ) {
        self.running_count.fetch_sub(1, Ordering::Relaxed);
        self.finish_job(id, description, status, error).await;
        self.emit_full_status(None).await;
        self.notify.notify_waiters();
    }

    async fn emit_full_status(&self, current_description: Option<String>) {
        let (active_jobs, jobs) = {
            let state = self.state.lock().await;
            let mut jobs: Vec<CloudSyncJobInfo> = Vec::new();
            jobs.extend(state.running_jobs.iter().cloned());
            for h in state.history.iter().rev() {
                jobs.push(h.clone());
            }
            let active = self.running_count.load(Ordering::Relaxed);
            (active, jobs)
        };

        if let Err(err) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.emitter.emit_status(&CloudSyncStatus {
                active_jobs,
                current_description,
                jobs,
            });
        })) {
            warn!(
                target: "rgsm::cloud::task_manager",
                "Failed to emit cloud sync status: {err:?}"
            );
        }
    }

    async fn finish_job(
        self: &Arc<Self>,
        id: u64,
        description: &str,
        status: CloudSyncJobStatus,
        error: Option<String>,
    ) {
        let auto_clear = matches!(
            status,
            CloudSyncJobStatus::Completed | CloudSyncJobStatus::Cancelled
        );
        let mut state = self.state.lock().await;
        state.running_jobs.retain(|j| j.id != id);
        state.history.push_back(CloudSyncJobInfo {
            id,
            description: description.to_string(),
            status,
            error,
        });
        while state.history.len() > MAX_HISTORY_SIZE {
            state.history.pop_front();
        }
        drop(state);

        // Failures remain available for inspection within the bounded history.
        if !auto_clear {
            return;
        }
        let this = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let mut state = this.state.lock().await;
            state.history.retain(|j| j.id != id);
            drop(state);
            this.emit_full_status(None).await;
        });
    }
}

impl Drop for CloudSyncTaskManager {
    fn drop(&mut self) {
        self.state.get_mut().manual_cancel_token.cancel();
    }
}

#[cfg(test)]
mod history_tests;

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopEmitter;

    impl SyncEventEmitter for NoopEmitter {
        fn emit_status(&self, _status: &CloudSyncStatus) {}
        fn emit_error(&self, _error: &CloudSyncError) {}
    }

    fn manager() -> Arc<CloudSyncTaskManager> {
        CloudSyncTaskManager::new(Arc::new(NoopEmitter))
    }

    #[tokio::test]
    async fn cancel_and_wait_does_not_return_while_a_manual_job_is_running() {
        let manager = manager();
        let (id, _) = manager.begin_manual_job("manual".to_string()).await;
        let finisher = manager.clone();
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            finisher
                .finish_manual_job(id, "manual", CloudSyncJobStatus::Cancelled, None)
                .await;
        });

        let result = manager.cancel_all_and_wait().await;

        assert!(matches!(result, CancelCloudSyncResult::Cancelled));
        assert_eq!(manager.running_count.load(Ordering::Acquire), 0);
    }
}
