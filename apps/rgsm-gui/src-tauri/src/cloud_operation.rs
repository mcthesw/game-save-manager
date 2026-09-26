use std::future::Future;
use std::sync::Arc;

use rgsm_core::cloud_sync::CloudSyncTaskManager;
use tauri::{AppHandle, Manager};
use tokio::sync::{Mutex, Notify};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Default)]
pub struct CloudOperationState {
    operation_lock: Arc<Mutex<()>>,
    wakeup: Arc<Notify>,
    background: Arc<std::sync::Mutex<CancellationToken>>,
}

pub async fn run<T>(app: &AppHandle, operation: impl Future<Output = T>) -> T {
    let state = app.state::<CloudOperationState>().inner().clone();
    state.run(operation).await
}

pub async fn run_after_cancelling<T>(app: &AppHandle, operation: impl Future<Output = T>) -> T {
    let manager = Arc::clone(app.state::<Arc<CloudSyncTaskManager>>().inner());
    let state = app.state::<CloudOperationState>().inner().clone();
    state.cancel_background();
    manager.cancel_all().await;
    run(app, async move {
        manager.cancel_all_and_wait().await;
        state.reset_background();
        let result = operation.await;
        state.request_sync();
        result
    })
    .await
}

impl CloudOperationState {
    pub async fn run<T>(&self, operation: impl Future<Output = T>) -> T {
        let _guard = self.operation_lock.lock().await;
        operation.await
    }

    /// A Notify permit coalesces requests and retains one wakeup during an active pass.
    pub fn request_sync(&self) {
        self.wakeup.notify_one();
    }

    pub async fn requested(&self) {
        self.wakeup.notified().await;
    }

    fn cancel_background(&self) {
        self.background
            .lock()
            .expect("background token lock")
            .cancel();
    }

    fn reset_background(&self) {
        *self.background.lock().expect("background token lock") = CancellationToken::new();
    }

    pub async fn run_background<F, Fut>(&self, operation: F)
    where
        F: FnOnce(CancellationToken) -> Fut,
        Fut: Future<Output = ()>,
    {
        let _guard = self.operation_lock.lock().await;
        let cancellation = self
            .background
            .lock()
            .expect("background token lock")
            .clone();
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {},
            _ = operation(cancellation.clone()) => {},
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::oneshot;

    use super::*;

    #[tokio::test]
    async fn requests_coalesce_and_survive_an_active_pass() {
        let state = CloudOperationState::default();
        state.request_sync();
        state.request_sync();
        tokio::time::timeout(Duration::from_millis(50), state.requested())
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(10), state.requested())
                .await
                .is_err()
        );
        state
            .run_background(|_| async {
                state.request_sync();
            })
            .await;
        tokio::time::timeout(Duration::from_millis(50), state.requested())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn cancelling_background_releases_cloud_boundary() {
        let state = CloudOperationState::default();
        let worker = state.clone();
        let (started, ready) = oneshot::channel();
        let task = tokio::spawn(async move {
            worker
                .run_background(|_| async {
                    started.send(()).unwrap();
                    std::future::pending::<()>().await;
                })
                .await;
        });
        ready.await.unwrap();
        state.cancel_background();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        state.reset_background();
        let mut ran = false;
        state
            .run_background(|_| async {
                ran = true;
            })
            .await;
        assert!(ran);
    }

    #[tokio::test]
    async fn operations_enter_one_at_a_time() {
        let state = CloudOperationState::default();
        let first_state = state.clone();
        let (first_started_tx, first_started_rx) = oneshot::channel();
        let (release_first_tx, release_first_rx) = oneshot::channel();
        let first = tokio::spawn(async move {
            first_state
                .run(async move {
                    first_started_tx.send(()).unwrap();
                    release_first_rx.await.unwrap();
                })
                .await;
        });
        first_started_rx.await.unwrap();

        let second_state = state.clone();
        let (second_started_tx, mut second_started_rx) = oneshot::channel();
        let second = tokio::spawn(async move {
            second_state
                .run(async move {
                    second_started_tx.send(()).unwrap();
                })
                .await;
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(50), &mut second_started_rx)
                .await
                .is_err()
        );

        release_first_tx.send(()).unwrap();
        first.await.unwrap();
        second_started_rx.await.unwrap();
        second.await.unwrap();
    }
}
