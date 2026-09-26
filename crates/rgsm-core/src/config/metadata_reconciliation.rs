use super::*;

impl OwnerStore {
    pub(crate) fn reconcile_game_metadata(
        &self,
        expected: &LocalState,
        remote: &SharedLibrary,
        completed: &[String],
        conflicts: &[String],
    ) -> Result<(), OwnerStoreError> {
        let mut owners = self.load()?;
        let state = &mut owners.local_state;
        if state.cloud_namespace_generation != CloudNamespaceGeneration::V2
            || state.cloud_library_id != expected.cloud_library_id
            || state.cloud_settings != expected.cloud_settings
            || state.current_device_id != expected.current_device_id
        {
            return Err(OwnerStoreError::SharedLibraryInputsChanged);
        }
        for id in completed {
            if let Some(current) = state.pending_game_metadata.get_mut(id) {
                if expected.pending_game_metadata.get(id) == Some(current) {
                    state.pending_game_metadata.remove(id);
                } else {
                    // A newer local edit still needs publication, using the
                    // version just published as its new common baseline.
                    current.base = remote
                        .games
                        .iter()
                        .find(|game| game.storage_key == *id)
                        .map(super::super::SharedGame::normalized_portable);
                    current.conflict = false;
                }
            }
        }
        for id in conflicts {
            if let Some(current) = state.pending_game_metadata.get_mut(id)
                && expected.pending_game_metadata.get(id) == Some(current)
            {
                current.conflict = true;
            }
        }
        let effective = remote.with_local_games(&state.pending_definitions());
        let current = owners.device_profiles[&state.current_device_id].clone();
        let profile = current.for_updated_library(&owners.shared_library, &effective);
        owners.accept_library(remote, &HashMap::from([(current.device.id, profile)]));
        self.write(&owners)
    }
}
