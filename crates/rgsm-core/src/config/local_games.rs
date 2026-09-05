use std::collections::{HashMap, HashSet};

use super::{
    ConfigurationOwners, DeviceProfile, LocalState, OwnershipError, SharedGame, SharedLibrary,
};
use crate::device::DeviceId;

impl ConfigurationOwners {
    pub(crate) fn validate_local_games(&self) -> Result<(), OwnershipError> {
        SharedLibrary {
            schema_version: self.shared_library.schema_version,
            games: self.local_state.local_games.clone(),
        }
        .validate()
    }

    pub(crate) fn preserve_local_scope(&mut self, previous: &ConfigurationOwners) {
        let local_ids = previous
            .local_state
            .local_games
            .iter()
            .map(|game| game.storage_key.as_str())
            .collect::<HashSet<_>>();
        let (local, shared) = std::mem::take(&mut self.shared_library.games)
            .into_iter()
            .partition(|game| local_ids.contains(game.storage_key.as_str()));
        self.local_state.local_games = local;
        self.shared_library.games = shared;
        // Saving the effective local view must not publish a pending local
        // definition over the separately cached cloud version with the same ID.
        self.shared_library.games.extend(
            previous
                .shared_library
                .games
                .iter()
                .filter(|game| local_ids.contains(game.storage_key.as_str()))
                .cloned(),
        );
    }

    pub(crate) fn connect_library(
        &mut self,
        library: &SharedLibrary,
        profiles: &HashMap<DeviceId, DeviceProfile>,
    ) {
        // At a new connection, every existing definition is a local candidate.
        // On ordinary refresh, only already retained candidates need a choice.
        self.local_state.local_games = self
            .shared_library
            .with_local_games(&self.local_state.local_games)
            .games;
        self.accept_library(library, profiles);
    }

    /// Cloud absence is not a deletion instruction. Retain omitted definitions
    /// locally, keeping their existing Device settings out of the cloud library.
    pub(crate) fn accept_library(
        &mut self,
        library: &SharedLibrary,
        profiles: &HashMap<DeviceId, DeviceProfile>,
    ) {
        let local_ids = self
            .local_state
            .local_games
            .iter()
            .map(|game| game.storage_key.clone())
            .collect::<HashSet<_>>();
        self.local_state.local_games = self
            .shared_library
            .with_local_games(&self.local_state.local_games)
            .games
            .into_iter()
            .filter(|game| {
                match library
                    .games
                    .iter()
                    .find(|shared| shared.storage_key == game.storage_key)
                {
                    None => true,
                    Some(shared) => {
                        local_ids.contains(&game.storage_key)
                            && game.normalized_portable() != shared.normalized_portable()
                    }
                }
            })
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

impl SharedLibrary {
    /// Local candidates take precedence in the compatibility view without
    /// changing the cached remote definitions or creating duplicate rows.
    pub(crate) fn with_local_games(&self, local: &[SharedGame]) -> Self {
        let mut library = self.clone();
        for game in local {
            if let Some(existing) = library
                .games
                .iter_mut()
                .find(|shared| shared.storage_key == game.storage_key)
            {
                *existing = game.clone();
            } else {
                library.games.push(game.clone());
            }
        }
        library
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

impl LocalState {
    pub(crate) fn local_game_ids(&self) -> std::collections::BTreeSet<String> {
        self.local_games
            .iter()
            .map(|game| game.storage_key.clone())
            .collect()
    }

    pub(crate) fn is_local_game(&self, game_id: &str) -> bool {
        self.local_games
            .iter()
            .any(|game| game.storage_key == game_id)
    }
}
