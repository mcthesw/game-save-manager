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
    if !DeviceProfileRepository::new(operator.clone(), 3)
        .list()
        .await?
        .iter()
        .any(|profile| profile.device.id == local_state.current_device_id)
    {
        return Err(CloudLibraryServiceError::DeviceReconnectRequired);
    }
    let remote = SharedLibraryRepository::new(operator.clone(), 3)
        .load()
        .await?;
    if remote != expected_library {
        let accepted_profile = expected_profile.for_shared_library(&remote);
        DeviceProfileRepository::new(operator, 3)
            .publish(&local_state.current_device_id, &accepted_profile)
            .await?;
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
    Ok(())
}
