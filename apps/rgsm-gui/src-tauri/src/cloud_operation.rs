use std::future::Future;
use std::sync::Arc;

use rgsm_core::cloud_sync::CloudSyncTaskManager;
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;

#[derive(Clone, Default)]
pub struct CloudOperationState {
    operation_lock: Arc<Mutex<()>>,
}

pub async fn run<T>(app: &AppHandle, operation: impl Future<Output = T>) -> T {
    let state = app.state::<CloudOperationState>().inner().clone();
    state.run(operation).await
}

pub async fn run_after_cancelling<T>(app: &AppHandle, operation: impl Future<Output = T>) -> T {
    let manager = Arc::clone(app.state::<Arc<CloudSyncTaskManager>>().inner());
    manager.cancel_all().await;
    run(app, async move {
        manager.cancel_all_and_wait().await;
        operation.await
    })
    .await
}

impl CloudOperationState {
    pub async fn run<T>(&self, operation: impl Future<Output = T>) -> T {
        let _guard = self.operation_lock.lock().await;
        operation.await
    }

    pub fn lock_handle(&self) -> Arc<Mutex<()>> {
        Arc::clone(&self.operation_lock)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::oneshot;

    use super::*;

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
