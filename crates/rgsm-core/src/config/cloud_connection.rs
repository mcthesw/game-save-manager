use super::*;

impl OwnerStore {
    pub(crate) fn connect_v2(
        &self,
        expected_library: &SharedLibrary,
        expected_profile: &DeviceProfile,
        expected_state: &LocalState,
        remote: &SharedLibrary,
        library_id: &str,
    ) -> Result<(), OwnerStoreError> {
        let mut owners =
            self.connected_owners(expected_library, expected_profile, expected_state, remote)?;
        owners.local_state.cloud_namespace_generation = CloudNamespaceGeneration::V2;
        owners.local_state.cloud_library_id = Some(library_id.to_string());
        self.write(&owners)
    }

    pub(crate) fn resolve_cloud_definitions(
        &self,
        expected_library: &SharedLibrary,
        expected_profile: &DeviceProfile,
        expected_state: &LocalState,
        accepted: &SharedLibrary,
        resolved_ids: &[String],
    ) -> Result<(), OwnerStoreError> {
        self.write(&self.resolved_owners(
            expected_library,
            expected_profile,
            expected_state,
            accepted,
            resolved_ids,
        )?)
    }

    pub(crate) fn connected_profile(
        &self,
        library: &SharedLibrary,
        profile: &DeviceProfile,
        state: &LocalState,
        remote: &SharedLibrary,
    ) -> Result<DeviceProfile, OwnerStoreError> {
        let owners = self.connected_owners(library, profile, state, remote)?;
        Ok(owners.device_profiles[&state.current_device_id]
            .without_local_games(&owners.local_state))
    }

    pub(crate) fn resolved_profile(
        &self,
        library: &SharedLibrary,
        profile: &DeviceProfile,
        state: &LocalState,
        accepted: &SharedLibrary,
        resolved_ids: &[String],
    ) -> Result<DeviceProfile, OwnerStoreError> {
        let owners = self.resolved_owners(library, profile, state, accepted, resolved_ids)?;
        Ok(owners.device_profiles[&state.current_device_id]
            .without_local_games(&owners.local_state))
    }

    fn connected_owners(
        &self,
        library: &SharedLibrary,
        profile: &DeviceProfile,
        state: &LocalState,
        remote: &SharedLibrary,
    ) -> Result<ConfigurationOwners, OwnerStoreError> {
        let mut owners = self.connection_inputs(library, profile, state)?;
        let accepted = profile.for_shared_library(remote);
        owners.connect_library(
            remote,
            &HashMap::from([(accepted.device.id.clone(), accepted)]),
        );
        Ok(owners)
    }

    fn resolved_owners(
        &self,
        expected_library: &SharedLibrary,
        expected_profile: &DeviceProfile,
        expected_state: &LocalState,
        accepted: &SharedLibrary,
        resolved_ids: &[String],
    ) -> Result<ConfigurationOwners, OwnerStoreError> {
        let mut owners =
            self.connection_inputs(expected_library, expected_profile, expected_state)?;
        if expected_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(OwnerStoreError::JoinInputsChanged);
        }
        owners.local_state.local_games.retain(|game| {
            !resolved_ids.contains(&game.storage_key)
                || !accepted
                    .games
                    .iter()
                    .any(|remote| remote.storage_key == game.storage_key)
        });
        let profile = expected_profile.for_shared_library(accepted);
        owners.accept_library(
            accepted,
            &HashMap::from([(profile.device.id.clone(), profile)]),
        );
        Ok(owners)
    }

    fn connection_inputs(
        &self,
        expected_library: &SharedLibrary,
        expected_profile: &DeviceProfile,
        expected_state: &LocalState,
    ) -> Result<ConfigurationOwners, OwnerStoreError> {
        let owners = self.load()?;
        if owners.shared_library != *expected_library
            || owners.local_state.current_device_id != expected_state.current_device_id
            || owners.local_state.cloud_settings != expected_state.cloud_settings
            || owners.local_state.cloud_namespace_generation
                != expected_state.cloud_namespace_generation
            || owners.local_state.cloud_library_id != expected_state.cloud_library_id
            || owners.local_state.local_games != expected_state.local_games
            || owners
                .device_profiles
                .get(&expected_state.current_device_id)
                != Some(expected_profile)
        {
            return Err(OwnerStoreError::JoinInputsChanged);
        }
        Ok(owners)
    }
}
