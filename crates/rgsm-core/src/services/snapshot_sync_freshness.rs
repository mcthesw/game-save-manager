//! Decides whether a background round can be skipped. A round is only needed
//! when the cloud library or the local sync inputs differ from what the last
//! successful round saw; checking the cloud costs one listing request instead
//! of a full round's reads.

use std::sync::Mutex;

use opendal::Operator;

use crate::cloud_sync::v2::library_signature;
use crate::config::cloud_bootstrap_inputs;

use super::cloud_library_target::cloud_library_target;
use super::snapshot_sync::local_fingerprint;

#[derive(Clone, PartialEq, Eq)]
struct Checkpoint {
    library_id: String,
    cloud: String,
    local: u64,
}

static LAST_ROUND: Mutex<Option<Checkpoint>> = Mutex::new(None);

pub(super) struct FreshnessProbe {
    library_id: String,
    operator: Operator,
    cloud: Option<String>,
}

impl FreshnessProbe {
    pub(super) async fn begin() -> Option<Self> {
        let (_, _, state) = cloud_bootstrap_inputs().ok()?;
        let library_id = state.cloud_library_id.clone()?;
        let operator = cloud_library_target(&state).ok()?.operator();
        // A failed or unreliable listing simply means "run the round".
        let cloud = library_signature(&operator).await.ok().flatten();
        Some(Self {
            library_id,
            operator,
            cloud,
        })
    }

    pub(super) fn matches_last_round(&self) -> bool {
        let (Some(cloud), Some(local)) = (&self.cloud, local_fingerprint()) else {
            return false;
        };
        let current = Checkpoint {
            library_id: self.library_id.clone(),
            cloud: cloud.clone(),
            local,
        };
        LAST_ROUND
            .lock()
            .is_ok_and(|last| last.as_ref() == Some(&current))
    }

    /// Remember a successful round. A round can also write the manifest, the
    /// device profile or descriptions without reporting an outcome, so the
    /// signature taken before it is not trusted; list again.
    pub(super) async fn record(self) {
        let cloud = library_signature(&self.operator).await.ok().flatten();
        let checkpoint = cloud
            .zip(local_fingerprint())
            .map(|(cloud, local)| Checkpoint {
                library_id: self.library_id,
                cloud,
                local,
            });
        if let Ok(mut last) = LAST_ROUND.lock() {
            *last = checkpoint;
        }
    }
}

pub(super) fn forget_last_round() {
    if let Ok(mut last) = LAST_ROUND.lock() {
        *last = None;
    }
}
