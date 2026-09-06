use std::{future::Future, time::Duration};

/// Some providers expose an empty or truncated object while overwriting JSON.
/// Retry only that incomplete-read case; repositories still decode, validate,
/// and report the final bytes using their own error types. Missing objects and
/// transport errors keep their original meaning.
pub(super) async fn read_complete_json<F, Fut, E>(
    mut read: F,
    max_attempts: usize,
) -> Result<Option<Vec<u8>>, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Option<Vec<u8>>, E>>,
{
    let attempts = max_attempts.max(1);
    for attempt in 0..attempts {
        let bytes = read().await?;
        let incomplete = bytes.as_deref().is_some_and(|bytes| {
            serde_json::from_slice::<serde::de::IgnoredAny>(bytes)
                .is_err_and(|error| error.is_eof())
        });
        if !incomplete || attempt + 1 == attempts {
            return Ok(bytes);
        }
        tokio::time::sleep(Duration::from_millis(10 << attempt.min(4))).await;
    }
    unreachable!("at least one read attempt")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, future::ready};

    #[tokio::test]
    async fn incomplete_json_is_retried_without_replacing_the_final_error_bytes() {
        let reads = Cell::new(0);
        let result = read_complete_json(
            || {
                reads.set(reads.get() + 1);
                ready(Ok::<_, ()>(Some(b"{\"games\":".to_vec())))
            },
            2,
        )
        .await
        .unwrap();
        assert_eq!(reads.get(), 2);
        assert_eq!(result, Some(b"{\"games\":".to_vec()));
    }

    #[tokio::test]
    async fn complete_missing_malformed_and_transport_results_are_not_retried() {
        for result in [
            Ok(Some(b"{}".to_vec())),
            Ok(None),
            Ok(Some(b"{broken".to_vec())),
            Err("offline"),
        ] {
            let reads = Cell::new(0);
            let actual = read_complete_json(
                || {
                    reads.set(reads.get() + 1);
                    ready(result.clone())
                },
                3,
            )
            .await;
            assert_eq!(reads.get(), 1);
            assert_eq!(actual, result);
        }
    }
}
