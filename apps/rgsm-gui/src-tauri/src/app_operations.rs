//! Admission at host entry points; accepted operations may finish their nested work.
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

#[derive(Default)]
struct Activity {
    paused: bool,
    running: usize,
}

#[derive(Default)]
struct Inner {
    activity: Mutex<Activity>,
    changed: Notify,
}

#[derive(Clone, Default)]
pub struct AppOperations(Arc<Inner>);

pub struct Operation(Arc<Inner>);
pub struct InstallationWindow(Arc<Inner>);

impl Operation {
    /// A disconnected HTTP caller must not release admission while its write still runs.
    pub fn spawn<F>(self, work: F) -> tokio::task::JoinHandle<F::Output>
    where
        F: std::future::Future + Send + 'static,
        F::Output: Send + 'static,
    {
        tokio::spawn(async move {
            let _operation = self;
            work.await
        })
    }
}

impl AppOperations {
    pub fn begin(&self) -> Option<Operation> {
        let mut activity = self.0.activity.lock().unwrap();
        if activity.paused {
            return None;
        }
        activity.running += 1;
        Some(Operation(self.0.clone()))
    }

    pub async fn pause_and_wait(&self) -> InstallationWindow {
        let window = InstallationWindow(self.0.clone());
        self.0.activity.lock().unwrap().paused = true;
        loop {
            let changed = self.0.changed.notified();
            if self.0.activity.lock().unwrap().running == 0 {
                return window;
            }
            changed.await;
        }
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        self.0.activity.lock().unwrap().running -= 1;
        self.0.changed.notify_one();
    }
}

impl Drop for InstallationWindow {
    fn drop(&mut self) {
        self.0.activity.lock().unwrap().paused = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn disconnected_caller_does_not_release_running_work() {
        let operations = AppOperations::default();
        let (finish, finished) = tokio::sync::oneshot::channel();
        let request = operations
            .begin()
            .unwrap()
            .spawn(async { finished.await.unwrap() });
        drop(request);
        let waiting = operations.pause_and_wait();
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut waiting)
                .await
                .is_err()
        );
        finish.send(()).unwrap();
        let _window = tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn installation_waits_for_accepted_work_and_rejects_new_work() {
        let operations = AppOperations::default();
        let active = operations.begin().unwrap();
        let pending = operations.pause_and_wait();
        tokio::pin!(pending);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut pending)
                .await
                .is_err()
        );
        assert!(operations.begin().is_none());
        drop(active);
        let window = tokio::time::timeout(Duration::from_secs(1), &mut pending)
            .await
            .unwrap();
        assert!(operations.begin().is_none());
        drop(window);
        assert!(operations.begin().is_some());
    }

    #[tokio::test]
    async fn cancelling_installation_wait_restores_admission() {
        let operations = AppOperations::default();
        let _active = operations.begin().unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(10), operations.pause_and_wait())
                .await
                .is_err()
        );
        assert!(operations.begin().is_some());
    }
}
