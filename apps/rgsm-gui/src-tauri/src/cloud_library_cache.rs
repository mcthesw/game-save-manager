//! The last cloud library state a reconciliation round produced. Pages show
//! it instead of querying the cloud themselves, so only a round (a new local
//! Snapshot, an explicit refresh, the startup check or the background timer)
//! ever reaches the provider.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rgsm_core::services::{CloudFailureKind, CloudLibrarySnapshot};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

#[derive(Default)]
struct Entry {
    snapshot: Option<CloudLibrarySnapshot>,
    connection: Option<String>,
    checked_at: Option<u64>,
    failure: Option<CloudFailure>,
}

static CACHE: Mutex<Option<Entry>> = Mutex::new(None);

/// Why the latest round could not read the cloud.
#[derive(Debug, Clone, Serialize, Type, utoipa::ToSchema)]
pub struct CloudFailure {
    pub kind: CloudFailureKind,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Type, utoipa::ToSchema)]
pub struct CachedCloudLibrary {
    pub snapshot: Option<CloudLibrarySnapshot>,
    /// Unix milliseconds of the last successful check, including checks that
    /// found nothing new.
    pub checked_at: Option<u64>,
    /// Set until a later round succeeds; the snapshot above is then the last known state.
    pub failure: Option<CloudFailure>,
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

/// A changed connection must not show the previous library's state.
fn connection_key() -> Option<String> {
    let config = rgsm_core::config::get_config().ok()?;
    serde_json::to_string(&config.settings.cloud_settings).ok()
}

/// The entry for the current connection, replacing one left by another.
fn current_entry(cache: &mut Option<Entry>) -> &mut Entry {
    let current = connection_key();
    if cache
        .as_ref()
        .is_none_or(|entry| entry.connection != current)
    {
        *cache = Some(Entry {
            connection: current,
            ..Entry::default()
        });
    }
    cache.as_mut().expect("entry was just created")
}

/// Call inside the reconciliation round whose reads it reuses.
pub async fn store(app: &AppHandle) -> Result<(), String> {
    let service = rgsm_core::services::ServiceContext::new(
        app.state::<crate::hooks::HookPipelineState>().snapshot(),
    );
    let snapshot = service
        .cloud_library_snapshot()
        .await
        .map_err(|error| rgsm_core::services::cloud_error_message(&error))?;
    if let Ok(mut cache) = CACHE.lock() {
        let entry = current_entry(&mut cache);
        entry.snapshot = Some(snapshot);
        entry.checked_at = Some(now_millis());
        entry.failure = None;
    }
    Ok(())
}

/// A check found the cloud unchanged: the cached state is still current.
pub fn mark_checked() {
    if let Ok(mut cache) = CACHE.lock() {
        let entry = current_entry(&mut cache);
        entry.checked_at = Some(now_millis());
        entry.failure = None;
    }
}

pub fn record_failure(error: &(dyn std::error::Error + 'static)) {
    if let Ok(mut cache) = CACHE.lock() {
        current_entry(&mut cache).failure = Some(CloudFailure {
            kind: rgsm_core::services::cloud_failure_kind(error),
            message: rgsm_core::services::cloud_error_message(error),
        });
    }
}

pub fn get() -> CachedCloudLibrary {
    let current = connection_key();
    match CACHE.lock().as_deref() {
        Ok(Some(entry)) if entry.connection == current => CachedCloudLibrary {
            snapshot: entry.snapshot.clone(),
            checked_at: entry.checked_at,
            failure: entry.failure.clone(),
        },
        _ => CachedCloudLibrary {
            snapshot: None,
            checked_at: None,
            failure: None,
        },
    }
}
