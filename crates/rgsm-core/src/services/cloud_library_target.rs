use opendal::Operator;

use crate::cloud_sync::CloudSyncSessionConfig;
use crate::cloud_sync::v2::{CloudLibraryTarget, CloudNamespaceError};
use crate::config::{CloudNamespaceGeneration, LocalState};
use crate::preclude::BackendError;

pub(crate) fn cloud_library_target(
    local_state: &LocalState,
) -> Result<CloudLibraryTarget, BackendError> {
    if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
        return Err(CloudNamespaceError::LocalLibraryIdMissing.into());
    }
    let library_id = local_state
        .cloud_library_id
        .as_deref()
        .ok_or(CloudNamespaceError::LocalLibraryIdMissing)?;
    let operator = CloudSyncSessionConfig::from(&local_state.cloud_settings).get_op()?;
    Ok(CloudLibraryTarget::new(operator, library_id))
}

pub(crate) async fn bound_v2_operator(local_state: &LocalState) -> Result<Operator, BackendError> {
    Ok(cloud_library_target(local_state)?.verify().await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud_sync::v2::{CloudNamespaceDescriptor, V2_NAMESPACE_DESCRIPTOR_PATH};
    use crate::cloud_sync::{Backend, CloudSettings};
    use crate::config::{Config, ConfigurationOwners};

    const LOCAL_LIBRARY_ID: &str = "11111111-1111-4111-8111-111111111111";
    const REMOTE_LIBRARY_ID: &str = "22222222-2222-4222-8222-222222222222";

    #[tokio::test]
    async fn bound_operator_rejects_a_different_cloud_library() {
        let root = temp_dir::TempDir::new().unwrap();
        let device_id = "device".to_string();
        let mut state =
            ConfigurationOwners::from_legacy(&Config::default(), &device_id).local_state;
        state.cloud_namespace_generation = CloudNamespaceGeneration::V2;
        state.cloud_library_id = Some(LOCAL_LIBRARY_ID.to_string());
        state.cloud_settings = CloudSettings {
            backend: Backend::Fs,
            root_path: root.path().to_string_lossy().into_owned(),
            ..CloudSettings::default()
        };
        let operator = CloudSyncSessionConfig::from(&state.cloud_settings)
            .get_op()
            .unwrap();
        operator
            .write(
                V2_NAMESPACE_DESCRIPTOR_PATH,
                serde_json::to_vec(&CloudNamespaceDescriptor::with_library_id(
                    REMOTE_LIBRARY_ID,
                ))
                .unwrap(),
            )
            .await
            .unwrap();

        assert!(matches!(
            bound_v2_operator(&state).await,
            Err(BackendError::CloudNamespace(
                CloudNamespaceError::LibraryIdentityMismatch
            ))
        ));

        operator
            .write(
                V2_NAMESPACE_DESCRIPTOR_PATH,
                serde_json::to_vec(&CloudNamespaceDescriptor::with_library_id(LOCAL_LIBRARY_ID))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(bound_v2_operator(&state).await.is_ok());
    }
}
