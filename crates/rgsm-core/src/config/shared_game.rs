use std::hash::Hasher;

use xxhash_rust::xxh3::Xxh3;

use super::{SharedGame, SharedSaveUnitSource};

impl SharedGame {
    /// Clone into the canonical comparison shape without rewriting semantic
    /// identity, display, path-pattern, or Save Unit values.
    pub fn normalized_portable(&self) -> Self {
        let mut normalized = self.clone();
        // Retention is shared Cloud Library policy, not part of a portable
        // Game definition. A joining Device inherits it from the cloud side.
        normalized.snapshot_retention = None;
        for unit in &mut normalized.save_units {
            if let SharedSaveUnitSource::ManifestPattern { constraints, .. } = &mut unit.source {
                constraints
                    .alternatives
                    .sort_by_key(|condition| (condition.os, condition.store));
                constraints.alternatives.dedup();
            }
        }
        normalized
    }

    pub fn portable_fingerprint(&self) -> Result<String, serde_json::Error> {
        let bytes = serde_json::to_vec(&self.normalized_portable())?;
        let mut hasher = Xxh3::new();
        hasher.write(&bytes);
        Ok(format!("{:016x}", hasher.finish()))
    }
}
