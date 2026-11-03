//! Progress of the Cloud transfers a round performs. Transfers run deep inside
//! coordinators and materializers, so they report to the reporter installed for
//! the current task instead of every layer passing a handle down.

use std::future::Future;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema,
)]
pub struct TransferProgress {
    pub items_done: u32,
    /// Grows as a round discovers more Archives to upload.
    pub items_total: u32,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

pub struct TransferReporter {
    state: Mutex<TransferProgress>,
    sink: Box<dyn Fn(TransferProgress) + Send + Sync>,
}

impl TransferReporter {
    pub fn new(sink: impl Fn(TransferProgress) + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(TransferProgress::default()),
            sink: Box::new(sink),
        })
    }

    fn update(&self, change: impl FnOnce(&mut TransferProgress)) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        change(&mut state);
        (self.sink)(*state);
    }
}

tokio::task_local! {
    static REPORTER: Arc<TransferReporter>;
}

pub async fn with_transfer_reporter<F: Future>(
    reporter: Arc<TransferReporter>,
    work: F,
) -> F::Output {
    REPORTER.scope(reporter, work).await
}

fn report(change: impl FnOnce(&mut TransferProgress)) {
    let _ = REPORTER.try_with(|reporter| reporter.update(change));
}

/// Announce Archives that are about to move.
pub(crate) fn plan(items: u32, bytes: u64) {
    report(|progress| {
        progress.items_total += items;
        progress.bytes_total += bytes;
    });
}

pub(crate) fn advance(bytes: u64) {
    report(|progress| progress.bytes_done += bytes);
}

pub(crate) fn finish_item() {
    report(|progress| progress.items_done += 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn transfers_report_to_the_reporter_of_their_task_only() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let reporter = TransferReporter::new(move |progress| sink.lock().unwrap().push(progress));

        plan(1, 10);
        with_transfer_reporter(reporter, async {
            plan(2, 30);
            advance(10);
            finish_item();
        })
        .await;
        advance(5);

        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 3);
        assert_eq!(
            seen[2],
            TransferProgress {
                items_done: 1,
                items_total: 2,
                bytes_done: 10,
                bytes_total: 30
            }
        );
    }
}
