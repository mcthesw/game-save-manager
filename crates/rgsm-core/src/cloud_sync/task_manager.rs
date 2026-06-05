use std::collections::VecDeque;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::sync::{Mutex, Notify, Semaphore};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use super::state_recording::{log_config_sync_failure, log_game_sync_failure};
use super::sync_state::{
    GameSyncState, PendingAction, SyncResult, build_game_sync_state, update_config_sync_state,
    update_game_sync_state, with_sync_state,
};
use crate::backup::GameSnapshots;
use crate::cloud_sync::transfer::CloudTransfer;
use crate::cloud_sync::{Backend, session_from_backend, upload_config, upload_game_snapshots};
use crate::config::get_config;
use crate::hooks::SyncJobQueue;
use crate::preclude::*;

/// Trait for emitting cloud sync events to the frontend.
///
/// GUI implements this using Tauri's event system; CLI might log to stdout.
pub trait SyncEventEmitter: Send + Sync {
    fn emit_status(&self, status: &CloudSyncStatus);
    fn emit_error(&self, error: &CloudSyncError);
}

const TASK_LEVEL_MAX_RETRIES: u8 = 2;
const MAX_HISTORY_SIZE: usize = 20;

#[derive(Debug, Clone)]
pub enum CloudSyncJob {
    UploadSnapshot {
        backend: Backend,
        game_name: String,
        storage_key: String,
        snapshots: GameSnapshots,
        local_zip_path: PathBuf,
        remote_zip_path: String,
    },
    UploadMetadata {
        backend: Backend,
        game_name: String,
        storage_key: String,
        snapshots: GameSnapshots,
    },
    DeleteSnapshotAndUploadMetadata {
        backend: Backend,
        game_name: String,
        storage_key: String,
        snapshots: GameSnapshots,
        remote_zip_path: String,
    },
    DeleteFilesAndUploadMetadata {
        backend: Backend,
        game_name: String,
        storage_key: String,
        snapshots: GameSnapshots,
        remote_zip_paths: Vec<String>,
    },
    DeleteGameAndUploadConfig {
        backend: Backend,
        game_name: String,
        remote_game_dir_path: String,
    },
    UploadConfig {
        backend: Backend,
        context: String,
    },
}

impl CloudSyncJob {
    fn description(&self) -> String {
        match self {
            CloudSyncJob::UploadSnapshot { game_name, .. } => {
                format!("Uploading snapshot for {game_name}")
            }
            CloudSyncJob::UploadMetadata { game_name, .. } => {
                format!("Uploading metadata for {game_name}")
            }
            CloudSyncJob::DeleteSnapshotAndUploadMetadata { game_name, .. } => {
                format!("Deleting remote snapshot for {game_name}")
            }
            CloudSyncJob::DeleteFilesAndUploadMetadata { game_name, .. } => {
                format!("Cleaning old backups for {game_name}")
            }
            CloudSyncJob::DeleteGameAndUploadConfig { game_name, .. } => {
                format!("Deleting game {game_name} from cloud")
            }
            CloudSyncJob::UploadConfig { context, .. } => {
                format!("Uploading cloud config ({context})")
            }
        }
    }

    fn game_name(&self) -> Option<String> {
        match self {
            CloudSyncJob::UploadSnapshot { game_name, .. }
            | CloudSyncJob::UploadMetadata { game_name, .. }
            | CloudSyncJob::DeleteSnapshotAndUploadMetadata { game_name, .. }
            | CloudSyncJob::DeleteFilesAndUploadMetadata { game_name, .. }
            | CloudSyncJob::DeleteGameAndUploadConfig { game_name, .. } => Some(game_name.clone()),
            CloudSyncJob::UploadConfig { .. } => None,
        }
    }

    fn backend(&self) -> &Backend {
        match self {
            CloudSyncJob::UploadSnapshot { backend, .. }
            | CloudSyncJob::UploadMetadata { backend, .. }
            | CloudSyncJob::DeleteSnapshotAndUploadMetadata { backend, .. }
            | CloudSyncJob::DeleteFilesAndUploadMetadata { backend, .. }
            | CloudSyncJob::DeleteGameAndUploadConfig { backend, .. }
            | CloudSyncJob::UploadConfig { backend, .. } => backend,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum CloudSyncJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CloudSyncJobInfo {
    pub id: u64,
    pub description: String,
    pub status: CloudSyncJobStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CloudSyncStatus {
    pub active_jobs: usize,
    pub current_description: Option<String>,
    pub jobs: Vec<CloudSyncJobInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CloudSyncError {
    pub game_name: Option<String>,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CancelCloudSyncResult {
    Cancelled,
    NoActiveOperations,
}

#[derive(Debug)]
struct QueuedJob {
    id: u64,
    job: CloudSyncJob,
}

#[derive(Debug)]
struct CloudSyncState {
    queue: VecDeque<QueuedJob>,
    queue_cancel_token: CancellationToken,
    manual_cancel_token: CancellationToken,
    shutdown: bool,
    next_id: u64,
    running_jobs: Vec<CloudSyncJobInfo>,
    history: VecDeque<CloudSyncJobInfo>,
}

impl Default for CloudSyncState {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
            queue_cancel_token: CancellationToken::new(),
            manual_cancel_token: CancellationToken::new(),
            shutdown: false,
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

    pub async fn run(self: Arc<Self>) {
        self.run_worker().await;
    }

    pub async fn enqueue(&self, job: CloudSyncJob) {
        {
            let mut state = self.state.lock().await;
            let id = state.next_id;
            state.next_id += 1;
            state.queue.push_back(QueuedJob { id, job });
        }

        self.notify.notify_one();
        self.emit_full_status(Some("Queued cloud sync job".to_string()))
            .await;
    }

    pub async fn enqueue_config_upload_if_enabled(
        &self,
        config: &crate::config::Config,
        context: impl Into<String>,
    ) {
        let backend = match &config.settings.cloud_settings.backend {
            Backend::Disabled => return,
            backend => backend.clone(),
        };

        self.enqueue(CloudSyncJob::UploadConfig {
            backend,
            context: context.into(),
        })
        .await;
    }

    pub async fn cancel_all(&self) -> CancelCloudSyncResult {
        let mut had_active = self.running_count.load(Ordering::Relaxed) > 0;
        {
            let mut state = self.state.lock().await;
            had_active |= !state.queue.is_empty();
            while let Some(queued) = state.queue.pop_front() {
                state.history.push_back(CloudSyncJobInfo {
                    id: queued.id,
                    description: queued.job.description(),
                    status: CloudSyncJobStatus::Cancelled,
                    error: None,
                });
                while state.history.len() > MAX_HISTORY_SIZE {
                    state.history.pop_front();
                }
            }
            state.queue_cancel_token.cancel();
            state.queue_cancel_token = CancellationToken::new();
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
        &self,
        id: u64,
        description: &str,
        status: CloudSyncJobStatus,
        error: Option<String>,
    ) {
        self.running_count.fetch_sub(1, Ordering::Relaxed);
        self.finish_job(id, description, status, error).await;
        self.emit_full_status(None).await;
    }

    async fn emit_full_status(&self, current_description: Option<String>) {
        let (active_jobs, jobs) = {
            let state = self.state.lock().await;
            let mut jobs: Vec<CloudSyncJobInfo> = Vec::new();
            jobs.extend(state.running_jobs.iter().cloned());
            for q in &state.queue {
                jobs.push(CloudSyncJobInfo {
                    id: q.id,
                    description: q.job.description(),
                    status: CloudSyncJobStatus::Queued,
                    error: None,
                });
            }
            for h in state.history.iter().rev() {
                jobs.push(h.clone());
            }
            let active = state.queue.len() + self.running_count.load(Ordering::Relaxed);
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

    fn emit_error(&self, game_name: Option<String>, error_message: String) {
        self.emitter.emit_error(&CloudSyncError {
            game_name,
            error: error_message,
        });
    }

    async fn mark_running(&self, id: u64, description: &str) {
        let mut state = self.state.lock().await;
        state.running_jobs.push(CloudSyncJobInfo {
            id,
            description: description.to_string(),
            status: CloudSyncJobStatus::Running,
            error: None,
        });
    }

    async fn finish_job(
        &self,
        id: u64,
        description: &str,
        status: CloudSyncJobStatus,
        error: Option<String>,
    ) {
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
    }

    async fn run_worker(self: Arc<Self>) {
        let max_concurrency = get_config()
            .map(|c| c.settings.cloud_settings.max_concurrency.max(1))
            .unwrap_or(1);
        let semaphore = Arc::new(Semaphore::new(max_concurrency));

        loop {
            let (queued_job, cancel_token) = {
                let mut state = self.state.lock().await;

                while state.queue.is_empty() && !state.shutdown {
                    drop(state);
                    self.notify.notified().await;
                    state = self.state.lock().await;
                }

                if state.shutdown {
                    return;
                }

                let Some(queued_job) = state.queue.pop_front() else {
                    continue;
                };
                (queued_job, state.queue_cancel_token.child_token())
            };

            let permit = semaphore.clone().acquire_owned().await;
            if permit.is_err() {
                return;
            }
            let permit = permit.unwrap();

            self.running_count.fetch_add(1, Ordering::Relaxed);

            let job_id = queued_job.id;
            let job = queued_job.job;
            let description = job.description();

            self.mark_running(job_id, &description).await;
            self.emit_full_status(Some(description.clone())).await;

            info!(
                target: "rgsm::cloud::task_manager",
                "Start cloud sync job: {description}"
            );

            let me = Arc::clone(&self);
            tokio::spawn(async move {
                let result = execute_job_with_retry(&job, &cancel_token).await;

                me.running_count.fetch_sub(1, Ordering::Relaxed);
                drop(permit);
                record_job_sync_state(&job, &result);

                match &result {
                    Ok(()) => {
                        info!(
                            target: "rgsm::cloud::task_manager",
                            "Finished cloud sync job: {description}"
                        );
                        me.finish_job(job_id, &description, CloudSyncJobStatus::Completed, None)
                            .await;
                    }
                    Err(CloudSyncExecuteError::Cancelled) => {
                        info!(
                            target: "rgsm::cloud::task_manager",
                            "Cancelled cloud sync job: {description}"
                        );
                        me.finish_job(job_id, &description, CloudSyncJobStatus::Cancelled, None)
                            .await;
                    }
                    Err(CloudSyncExecuteError::Backend(err)) => {
                        let err_str = err.to_string();
                        error!(
                            target: "rgsm::cloud::task_manager",
                            "Cloud sync job failed after retry: {description}: {err:?}"
                        );
                        me.finish_job(
                            job_id,
                            &description,
                            CloudSyncJobStatus::Failed,
                            Some(err_str.clone()),
                        )
                        .await;
                        me.emit_error(job.game_name(), err_str);
                    }
                }

                me.emit_full_status(None).await;
            });
        }
    }
}

#[async_trait::async_trait]
impl SyncJobQueue for CloudSyncTaskManager {
    async fn enqueue(&self, job: CloudSyncJob) {
        CloudSyncTaskManager::enqueue(self, job).await;
    }
}

impl Drop for CloudSyncTaskManager {
    fn drop(&mut self) {
        let state = self.state.get_mut();
        state.shutdown = true;
        state.queue_cancel_token.cancel();
        state.manual_cancel_token.cancel();
        self.notify.notify_waiters();
    }
}

fn record_job_sync_state(job: &CloudSyncJob, result: &Result<(), CloudSyncExecuteError>) {
    let Ok(session) = session_from_backend(job.backend()) else {
        return;
    };

    match job {
        CloudSyncJob::UploadSnapshot {
            game_name,
            snapshots,
            ..
        }
        | CloudSyncJob::UploadMetadata {
            game_name,
            snapshots,
            ..
        }
        | CloudSyncJob::DeleteSnapshotAndUploadMetadata {
            game_name,
            snapshots,
            ..
        }
        | CloudSyncJob::DeleteFilesAndUploadMetadata {
            game_name,
            snapshots,
            ..
        } => {
            let state = build_state_for_result(snapshots.current_device_head_cloned(), result);
            if let Err(CloudSyncExecuteError::Backend(err)) = result {
                log_game_sync_failure(
                    &session,
                    game_name,
                    "queued_game_upload",
                    PendingAction::RetryRequired,
                    &err.to_string(),
                );
            }
            if let Err(err) = with_sync_state(|sync_state| {
                update_game_sync_state(sync_state, &session, game_name, state);
            }) {
                warn!("Failed to record queue sync state for {game_name}: {err}");
            }
        }
        CloudSyncJob::DeleteGameAndUploadConfig { game_name, .. } => {
            let config_state = build_state_for_result(None, result);
            if let Err(CloudSyncExecuteError::Backend(err)) = result {
                log_game_sync_failure(
                    &session,
                    game_name,
                    "queued_delete_game",
                    PendingAction::RetryRequired,
                    &err.to_string(),
                );
                log_config_sync_failure(
                    &session,
                    "queued_delete_game_config_upload",
                    &err.to_string(),
                );
            }
            if let Err(err) = with_sync_state(|sync_state| {
                update_config_sync_state(sync_state, &session, config_state.clone());
                if result.is_ok() {
                    sync_state.games.remove(game_name);
                }
            }) {
                warn!("Failed to record config sync state for deleted game {game_name}: {err}");
            }
        }
        CloudSyncJob::UploadConfig { .. } => {
            let config_state = build_state_for_result(None, result);
            if let Err(CloudSyncExecuteError::Backend(err)) = result {
                log_config_sync_failure(&session, "queued_config_upload", &err.to_string());
            }
            if let Err(err) = with_sync_state(|sync_state| {
                update_config_sync_state(sync_state, &session, config_state);
            }) {
                warn!("Failed to record config upload state: {err}");
            }
        }
    }
}

fn build_state_for_result(
    head: Option<String>,
    result: &Result<(), CloudSyncExecuteError>,
) -> GameSyncState {
    match result {
        Ok(()) => {
            build_game_sync_state(head.clone(), head, SyncResult::Success, PendingAction::None)
        }
        Err(CloudSyncExecuteError::Cancelled) => {
            build_game_sync_state(head, None, SyncResult::Cancelled, PendingAction::None)
        }
        Err(CloudSyncExecuteError::Backend(err)) => build_game_sync_state(
            head,
            None,
            SyncResult::Error(err.to_string()),
            PendingAction::RetryRequired,
        ),
    }
}

enum CloudSyncExecuteError {
    Cancelled,
    Backend(BackendError),
}

async fn run_cancellable<T, F>(
    token: &CancellationToken,
    future: F,
) -> Result<T, CloudSyncExecuteError>
where
    F: Future<Output = Result<T, BackendError>>,
{
    tokio::select! {
        _ = token.cancelled() => Err(CloudSyncExecuteError::Cancelled),
        res = future => res.map_err(CloudSyncExecuteError::Backend),
    }
}

async fn execute_job_with_retry(
    job: &CloudSyncJob,
    token: &CancellationToken,
) -> Result<(), CloudSyncExecuteError> {
    let mut attempt: u8 = 0;

    loop {
        if token.is_cancelled() {
            return Err(CloudSyncExecuteError::Cancelled);
        }

        match execute_job_once(job, token).await {
            Ok(()) => return Ok(()),
            Err(CloudSyncExecuteError::Cancelled) => return Err(CloudSyncExecuteError::Cancelled),
            Err(CloudSyncExecuteError::Backend(err)) => {
                if attempt >= TASK_LEVEL_MAX_RETRIES {
                    return Err(CloudSyncExecuteError::Backend(err));
                }

                let wait_duration = if attempt == 0 {
                    Duration::from_secs(1)
                } else {
                    Duration::from_secs(3)
                };

                warn!(
                    target: "rgsm::cloud::task_manager",
                    "Cloud sync attempt {} failed, retrying in {:?}: {err:?}",
                    attempt + 1,
                    wait_duration
                );

                tokio::select! {
                    _ = token.cancelled() => return Err(CloudSyncExecuteError::Cancelled),
                    _ = sleep(wait_duration) => {}
                }

                attempt += 1;
            }
        }
    }
}

async fn execute_job_once(
    job: &CloudSyncJob,
    token: &CancellationToken,
) -> Result<(), CloudSyncExecuteError> {
    match job {
        CloudSyncJob::UploadSnapshot {
            backend,
            snapshots,
            storage_key,
            local_zip_path,
            remote_zip_path,
            ..
        } => {
            let op = backend.get_op().map_err(CloudSyncExecuteError::Backend)?;
            run_cancellable(token, upload_game_snapshots(&op, storage_key, snapshots)).await?;
            let transfer = CloudTransfer::new(&op);
            run_cancellable(
                token,
                transfer.upload_file_streaming(local_zip_path, remote_zip_path),
            )
            .await?;
            Ok(())
        }
        CloudSyncJob::UploadMetadata {
            backend,
            snapshots,
            storage_key,
            ..
        } => {
            let op = backend.get_op().map_err(CloudSyncExecuteError::Backend)?;
            run_cancellable(token, upload_game_snapshots(&op, storage_key, snapshots)).await?;
            Ok(())
        }
        CloudSyncJob::DeleteSnapshotAndUploadMetadata {
            backend,
            snapshots,
            storage_key,
            remote_zip_path,
            ..
        } => {
            let op = backend.get_op().map_err(CloudSyncExecuteError::Backend)?;
            run_cancellable(token, async {
                op.delete(remote_zip_path).await.map_err(BackendError::from)
            })
            .await?;
            run_cancellable(token, upload_game_snapshots(&op, storage_key, snapshots)).await?;
            Ok(())
        }
        CloudSyncJob::DeleteFilesAndUploadMetadata {
            backend,
            snapshots,
            storage_key,
            remote_zip_paths,
            ..
        } => {
            let op = backend.get_op().map_err(CloudSyncExecuteError::Backend)?;
            for remote_path in remote_zip_paths {
                run_cancellable(token, async {
                    op.delete(remote_path).await.map_err(BackendError::from)
                })
                .await?;
            }
            run_cancellable(token, upload_game_snapshots(&op, storage_key, snapshots)).await?;
            Ok(())
        }
        CloudSyncJob::DeleteGameAndUploadConfig {
            backend,
            remote_game_dir_path,
            ..
        } => {
            let op = backend.get_op().map_err(CloudSyncExecuteError::Backend)?;
            run_cancellable(token, async {
                op.remove_all(remote_game_dir_path)
                    .await
                    .map_err(BackendError::from)
            })
            .await?;
            run_cancellable(token, upload_config(&op)).await?;
            Ok(())
        }
        CloudSyncJob::UploadConfig { backend, .. } => {
            let op = backend.get_op().map_err(CloudSyncExecuteError::Backend)?;
            run_cancellable(token, upload_config(&op)).await?;
            Ok(())
        }
    }
}

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
    async fn config_upload_enqueue_skips_disabled_backend() {
        let manager = manager();
        let config = crate::config::Config::default();

        manager
            .enqueue_config_upload_if_enabled(&config, "config_migration")
            .await;

        let state = manager.state.lock().await;
        assert!(state.queue.is_empty());
    }

    #[tokio::test]
    async fn config_upload_enqueue_uses_config_backend() {
        let manager = manager();
        let mut config = crate::config::Config::default();
        config.settings.cloud_settings.backend = Backend::WebDAV {
            endpoint: "https://example.invalid/dav".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
        };

        manager
            .enqueue_config_upload_if_enabled(&config, "config_migration")
            .await;

        let state = manager.state.lock().await;
        assert_eq!(state.queue.len(), 1);
        match &state.queue[0].job {
            CloudSyncJob::UploadConfig {
                backend: Backend::WebDAV { endpoint, .. },
                context,
            } => {
                assert_eq!(endpoint, "https://example.invalid/dav");
                assert_eq!(context, "config_migration");
            }
            other => panic!("expected config upload job, got {other:?}"),
        }
    }
}
