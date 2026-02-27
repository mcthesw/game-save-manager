use std::collections::VecDeque;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::sync::{Mutex, Notify, Semaphore};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::backup::GameSnapshots;
use crate::cloud_sync::transfer::CloudTransfer;
use crate::cloud_sync::{Backend, upload_config, upload_game_snapshots};
use crate::config::get_config;
use crate::preclude::*;

const TASK_LEVEL_MAX_RETRIES: u8 = 2;

#[derive(Debug, Clone)]
pub enum CloudSyncJob {
    UploadSnapshot {
        backend: Backend,
        game_name: String,
        snapshots: GameSnapshots,
        local_zip_path: PathBuf,
        remote_zip_path: String,
    },
    UploadMetadata {
        backend: Backend,
        game_name: String,
        snapshots: GameSnapshots,
    },
    DeleteSnapshotAndUploadMetadata {
        backend: Backend,
        game_name: String,
        snapshots: GameSnapshots,
        remote_zip_path: String,
    },
    DeleteFilesAndUploadMetadata {
        backend: Backend,
        game_name: String,
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

const MAX_HISTORY_SIZE: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct CloudSyncStatus {
    pub active_jobs: usize,
    pub current_description: Option<String>,
    pub jobs: Vec<CloudSyncJobInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct CloudSyncError {
    pub game_name: Option<String>,
    pub error: String,
}

#[derive(Debug)]
struct QueuedJob {
    id: u64,
    job: CloudSyncJob,
}

#[derive(Debug)]
struct CloudSyncState {
    queue: VecDeque<QueuedJob>,
    cancel_token: CancellationToken,
    shutdown: bool,
    next_id: u64,
    running_jobs: Vec<CloudSyncJobInfo>,
    history: VecDeque<CloudSyncJobInfo>,
}

impl Default for CloudSyncState {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
            cancel_token: CancellationToken::new(),
            shutdown: false,
            next_id: 1,
            running_jobs: Vec::new(),
            history: VecDeque::new(),
        }
    }
}

pub struct CloudSyncTaskManager {
    app: AppHandle,
    state: Mutex<CloudSyncState>,
    running_count: AtomicUsize,
    notify: Notify,
}

impl CloudSyncTaskManager {
    pub fn new(app: &AppHandle) -> Arc<Self> {
        let manager = Arc::new(Self {
            app: app.clone(),
            state: Mutex::new(CloudSyncState::default()),
            running_count: AtomicUsize::new(0),
            notify: Notify::new(),
        });

        let worker = Arc::clone(&manager);
        tauri::async_runtime::spawn(async move {
            worker.run_worker().await;
        });

        manager
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

    pub async fn cancel_all(&self) {
        {
            let mut state = self.state.lock().await;
            // Mark queued jobs as cancelled in history
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
            state.cancel_token.cancel();
            state.cancel_token = CancellationToken::new();
        }

        self.notify.notify_one();
        self.emit_full_status(None).await;
    }

    async fn emit_full_status(&self, current_description: Option<String>) {
        let (active_jobs, jobs) = {
            let state = self.state.lock().await;
            let mut jobs: Vec<CloudSyncJobInfo> = Vec::new();
            // Running jobs first
            jobs.extend(state.running_jobs.iter().cloned());
            // Queued jobs
            for q in &state.queue {
                jobs.push(CloudSyncJobInfo {
                    id: q.id,
                    description: q.job.description(),
                    status: CloudSyncJobStatus::Queued,
                    error: None,
                });
            }
            // History (newest first)
            for h in state.history.iter().rev() {
                jobs.push(h.clone());
            }
            let active = state.queue.len() + self.running_count.load(Ordering::Relaxed);
            (active, jobs)
        };

        if let Err(err) = (CloudSyncStatus {
            active_jobs,
            current_description,
            jobs,
        })
        .emit(&self.app)
        {
            warn!(
                target: "rgsm::cloud::task_manager",
                "Failed to emit cloud sync status: {err:?}"
            );
        }
    }

    fn emit_error(&self, game_name: Option<String>, error_message: String) {
        if let Err(err) = (CloudSyncError {
            game_name,
            error: error_message,
        })
        .emit(&self.app)
        {
            warn!(
                target: "rgsm::cloud::task_manager",
                "Failed to emit cloud sync error: {err:?}"
            );
        }
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
        // Read max_concurrency from config, fall back to 1
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
                (queued_job, state.cancel_token.child_token())
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
            tauri::async_runtime::spawn(async move {
                let result = execute_job_with_retry(&job, &cancel_token).await;

                me.running_count.fetch_sub(1, Ordering::Relaxed);
                drop(permit);

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

impl Drop for CloudSyncTaskManager {
    fn drop(&mut self) {
        let state = self.state.get_mut();
        state.shutdown = true;
        state.cancel_token.cancel();
        self.notify.notify_waiters();
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
            Err(CloudSyncExecuteError::Cancelled) => {
                return Err(CloudSyncExecuteError::Cancelled);
            }
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
            local_zip_path,
            remote_zip_path,
            ..
        } => {
            let op = backend.get_op().map_err(CloudSyncExecuteError::Backend)?;
            run_cancellable(token, upload_game_snapshots(&op, snapshots.clone())).await?;
            let transfer = CloudTransfer::new(&op);
            run_cancellable(
                token,
                transfer.upload_file_streaming(local_zip_path, remote_zip_path),
            )
            .await?;
            Ok(())
        }
        CloudSyncJob::UploadMetadata {
            backend, snapshots, ..
        } => {
            let op = backend.get_op().map_err(CloudSyncExecuteError::Backend)?;
            run_cancellable(token, upload_game_snapshots(&op, snapshots.clone())).await?;
            Ok(())
        }
        CloudSyncJob::DeleteSnapshotAndUploadMetadata {
            backend,
            snapshots,
            remote_zip_path,
            ..
        } => {
            let op = backend.get_op().map_err(CloudSyncExecuteError::Backend)?;
            run_cancellable(token, async {
                op.delete(remote_zip_path).await.map_err(BackendError::from)
            })
            .await?;
            run_cancellable(token, upload_game_snapshots(&op, snapshots.clone())).await?;
            Ok(())
        }
        CloudSyncJob::DeleteFilesAndUploadMetadata {
            backend,
            snapshots,
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
            run_cancellable(token, upload_game_snapshots(&op, snapshots.clone())).await?;
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
