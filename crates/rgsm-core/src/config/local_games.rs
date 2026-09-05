use std::collections::{HashMap, HashSet};

use super::{ConfigurationOwners, DeviceProfile, LocalState, OwnershipError, SharedLibrary};
use crate::device::DeviceId;

impl ConfigurationOwners {
    pub(crate) fn validate_local_games(&self) -> Result<(), OwnershipError> {
        SharedLibrary {
            schema_version: self.shared_library.schema_version,
            games: self.local_state.local_games.clone(),
        }
        .validate()?;
        for game in &self.local_state.local_games {
            if self
                .shared_library
                .games
                .iter()
                .any(|shared| shared.storage_key == game.storage_key)
            {
                return Err(OwnershipError::DuplicateGameOwnership(
                    game.storage_key.clone(),
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn preserve_local_scope(&mut self, previous: &LocalState) {
        let local_ids = previous
            .local_games
            .iter()
            .map(|game| game.storage_key.as_str())
            .collect::<HashSet<_>>();
        let (local, shared) = std::mem::take(&mut self.shared_library.games)
            .into_iter()
            .partition(|game| local_ids.contains(game.storage_key.as_str()));
        self.local_state.local_games = local;
        self.shared_library.games = shared;
    }

    /// Cloud absence is not a deletion instruction. Retain omitted definitions
    /// locally, keeping their existing Device settings out of the cloud library.
    pub(crate) fn accept_library(
        &mut self,
        library: &SharedLibrary,
        profiles: &HashMap<DeviceId, DeviceProfile>,
    ) {
        let shared_ids = library
            .games
            .iter()
            .map(|game| game.storage_key.as_str())
            .collect::<HashSet<_>>();
        self.local_state.local_games = self
            .local_state
            .local_games
            .iter()
            .chain(self.shared_library.games.iter())
            .filter(|game| !shared_ids.contains(game.storage_key.as_str()))
            .cloned()
            .collect();
        for (device_id, accepted) in profiles {
            let mut profile = accepted.clone();
            if let Some(previous) = self.device_profiles.get(device_id) {
                for game in &self.local_state.local_games {
                    if let Some(settings) = previous.games.get(&game.storage_key) {
                        profile
                            .games
                            .insert(game.storage_key.clone(), settings.clone());
                    }
                }
            }
            self.device_profiles.insert(device_id.clone(), profile);
        }
        // A definition retained from a previously connected library must not
        // continue publishing snapshots to the new library automatically.
        for profile in self.device_profiles.values_mut() {
            for game in &self.local_state.local_games {
                if let Some(settings) = profile.games.get_mut(&game.storage_key) {
                    settings.cloud_sync_enabled = false;
                }
            }
        }
        self.shared_library = library.clone();
    }
}

impl DeviceProfile {
    /// Explicit projection used by application services before publication.
    /// Local paths and shortcuts for unshared Games stay in the owner store.
    pub(crate) fn without_local_games(&self, local: &LocalState) -> Self {
        let mut profile = self.clone();
        for game in &local.local_games {
            profile.remove_game_state(&game.storage_key, &game.name);
        }
        profile
    }
}
