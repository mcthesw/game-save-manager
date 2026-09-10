use super::*;
use tokio::sync::watch;

struct StatusEmitter(watch::Sender<CloudSyncStatus>);

impl SyncEventEmitter for StatusEmitter {
    fn emit_status(&self, status: &CloudSyncStatus) {
        self.0.send_replace(status.clone());
    }

    fn emit_error(&self, _error: &CloudSyncError) {}
}

#[tokio::test]
async fn failed_jobs_remain_visible_after_normal_history_cleanup() {
    let (sender, mut receiver) = watch::channel(CloudSyncStatus {
        active_jobs: 0,
        current_description: None,
        jobs: Vec::new(),
    });
    let manager = CloudSyncTaskManager::new(Arc::new(StatusEmitter(sender)));
    for status in [
        CloudSyncJobStatus::Failed,
        CloudSyncJobStatus::Completed,
        CloudSyncJobStatus::Cancelled,
    ] {
        let error = matches!(status, CloudSyncJobStatus::Failed).then(|| "offline".to_string());
        let (id, _) = manager.begin_manual_job("transfer".into()).await;
        manager
            .finish_manual_job(id, "transfer", status, error)
            .await;
    }

    // Observe the actual cleanup event, not an assumed scheduler delay.
    let cleaned = tokio::time::timeout(
        Duration::from_secs(20),
        receiver.wait_for(|status| {
            !status.jobs.iter().any(|job| {
                matches!(
                    job.status,
                    CloudSyncJobStatus::Completed | CloudSyncJobStatus::Cancelled
                )
            })
        }),
    )
    .await
    .expect("completed and cancelled jobs should be cleaned up")
    .unwrap()
    .clone();
    assert_eq!(cleaned.active_jobs, 0);
    assert_eq!(cleaned.jobs.len(), 1);
    assert!(matches!(cleaned.jobs[0].status, CloudSyncJobStatus::Failed));
    assert_eq!(cleaned.jobs[0].error.as_deref(), Some("offline"));

    // Retained failures still obey the existing bounded history policy.
    for _ in 0..MAX_HISTORY_SIZE {
        let (id, _) = manager.begin_manual_job("next transfer".into()).await;
        manager
            .finish_manual_job(
                id,
                "next transfer",
                CloudSyncJobStatus::Failed,
                Some("next error".into()),
            )
            .await;
    }
    let latest = receiver.borrow();
    assert_eq!(latest.jobs.len(), MAX_HISTORY_SIZE);
    assert!(latest.jobs.iter().all(|job| job.id != cleaned.jobs[0].id));
    assert_eq!(latest.jobs[0].description, "next transfer");
    assert_eq!(latest.jobs[0].error.as_deref(), Some("next error"));
}
