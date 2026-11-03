//! Reads of the small shared metadata objects (Manifest, deletion registry,
//! namespace descriptor) repeat many times inside one reconciliation round,
//! and each one is a network request. A round scope serves repeated reads
//! from memory so the provider sees every object once per round.
//!
//! The cache only exists inside [`with_read_round`], is dropped with it, and is
//! bounded in age so a long upload cannot keep serving a stale answer. Writers
//! call [`invalidate`], and write guards use fresh reads instead of the cache.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::time::{Duration, Instant};

const MAX_ENTRY_AGE: Duration = Duration::from_secs(20);

type Entry = (Instant, Option<Vec<u8>>);

tokio::task_local! {
    static ROUND: RefCell<HashMap<String, Entry>>;
}

/// Run `work` with a read cache. Nested calls share the outer round.
pub async fn with_read_round<F: Future>(work: F) -> F::Output {
    if ROUND.try_with(|_| ()).is_ok() {
        return work.await;
    }
    ROUND.scope(RefCell::new(HashMap::new()), work).await
}

pub(super) async fn cached_read<F, Fut, E>(
    key: Option<&str>,
    fetch: F,
) -> Result<Option<Vec<u8>>, E>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Option<Vec<u8>>, E>>,
{
    let Some(key) = key else {
        return fetch().await;
    };
    let hit = ROUND
        .try_with(|cache| {
            cache
                .borrow()
                .get(key)
                .filter(|(stored_at, _)| stored_at.elapsed() < MAX_ENTRY_AGE)
                .map(|(_, bytes)| bytes.clone())
        })
        .ok()
        .flatten();
    if let Some(bytes) = hit {
        return Ok(bytes);
    }
    let bytes = fetch().await?;
    let _ = ROUND.try_with(|cache| {
        cache
            .borrow_mut()
            .insert(key.to_string(), (Instant::now(), bytes.clone()));
    });
    Ok(bytes)
}

pub(super) fn invalidate(key: &str) {
    let _ = ROUND.try_with(|cache| {
        cache.borrow_mut().remove(key);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::future::ready;

    #[tokio::test]
    async fn reads_repeat_outside_a_round() {
        let fetches = Cell::new(0);
        for _ in 0..2 {
            cached_read(Some("k"), || {
                fetches.set(fetches.get() + 1);
                ready(Ok::<_, ()>(Some(vec![1])))
            })
            .await
            .unwrap();
        }
        assert_eq!(fetches.get(), 2);
    }

    #[tokio::test]
    async fn a_round_serves_repeated_reads_until_the_object_is_written() {
        let fetches = Cell::new(0);
        let read = || {
            cached_read(Some("k"), || {
                fetches.set(fetches.get() + 1);
                ready(Ok::<_, ()>(Some(vec![fetches.get() as u8])))
            })
        };
        with_read_round(async {
            assert_eq!(read().await.unwrap(), Some(vec![1]));
            assert_eq!(read().await.unwrap(), Some(vec![1]));
            invalidate("k");
            assert_eq!(read().await.unwrap(), Some(vec![2]));
            with_read_round(async {
                assert_eq!(read().await.unwrap(), Some(vec![2]));
            })
            .await;
        })
        .await;
        assert_eq!(fetches.get(), 2);
    }

    #[tokio::test]
    async fn failures_are_never_cached() {
        let fetches = Cell::new(0);
        with_read_round(async {
            for _ in 0..2 {
                let result = cached_read(Some("k"), || {
                    fetches.set(fetches.get() + 1);
                    ready(Err(opendal::Error::new(
                        opendal::ErrorKind::Unexpected,
                        "x",
                    )))
                })
                .await;
                assert!(result.is_err());
            }
        })
        .await;
        assert_eq!(fetches.get(), 2);
    }
}
