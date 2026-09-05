use crate::cloud_sync::v2::{DeviceProfileRepository, SharedLibraryRepository};
use crate::config::{
    CloudNamespaceGeneration, accept_remote_shared_library, cloud_bootstrap_inputs,
};

use super::{CloudLibraryServiceError, cloud_library_target::bound_v2_operator};

/// Accept portable definitions for a registered Device while keeping configuration
/// reconciliation separate from archive queries and retention policy changes.
pub(super) async fn refresh_shared_library() -> Result<(), CloudLibraryServiceError> {
    let (expected_library, expected_profile, local_state) = cloud_bootstrap_inputs()?;
    if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
        return Ok(());
    }
    let operator = bound_v2_operator(&local_state).await?;
    let profiles = DeviceProfileRepository::new(operator.clone(), 3)
        .list()
        .await?;
    let Some(published) = profiles
        .iter()
        .find(|profile| profile.device.id == local_state.current_device_id)
    else {
        return Err(CloudLibraryServiceError::DeviceReconnectRequired);
    };
    let remote = SharedLibraryRepository::new(operator.clone(), 3)
        .load()
        .await?;
    if remote != expected_library {
        let accepted_profile = expected_profile.for_shared_library(&remote);
        accept_remote_shared_library(
            &expected_library,
            &expected_profile,
            &remote,
            &accepted_profile,
            local_state
                .cloud_library_id
                .as_deref()
                .ok_or(CloudLibraryServiceError::ActiveLibraryUnavailable)?,
        )?;
    }
    let (_, current, state) = cloud_bootstrap_inputs()?;
    let desired = current.without_local_games(&state);
    if *published != desired {
        DeviceProfileRepository::new(operator, 3)
            .publish(&state.current_device_id, &desired)
            .await?;
    }
    Ok(())
}
