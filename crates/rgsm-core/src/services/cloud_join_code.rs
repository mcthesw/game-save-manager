use serde::Serialize;
use thiserror::Error;

use super::{CloudLibraryStatus, ServiceContext};
use crate::cloud_sync::v2::{
    CloudLibraryBootstrap, CloudNamespaceClassification, DeviceProfileRepository,
};
use crate::cloud_sync::{
    Backend, CloudSettings, CloudSyncSessionConfig,
    join_code::{CloudJoinCode, JoinCodeError},
};
use crate::config::{
    SharedLibrary, cloud_bootstrap_inputs, connect_cloud_library_local, connected_cloud_profile,
};

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CloudJoinPreview {
    pub backend: String,
    pub endpoint: String,
    pub root_path: String,
    pub bucket: Option<String>,
    pub library_id: String,
    pub game_count: usize,
}

#[derive(Debug, Error)]
pub enum CloudJoinError {
    #[error(transparent)]
    Code(#[from] JoinCodeError),
    #[error("{}", rust_i18n::t!("cloud_join.errors.not_connected"))]
    NotConnected,
    #[error("{}", rust_i18n::t!("cloud_join.errors.unavailable"))]
    Unavailable,
    #[error("{}", rust_i18n::t!("cloud_join.errors.library"))]
    InvalidLibrary,
    #[error("{}", rust_i18n::t!("cloud_join.errors.confirmation"))]
    ConfirmationRequired,
    #[error("{}", rust_i18n::t!("cloud_join.errors.registration"))]
    RegistrationFailed,
    #[error("{}", rust_i18n::t!("cloud_join.errors.local_changed"))]
    LocalSettingsChanged,
}

impl ServiceContext {
    pub async fn export_cloud_join_code(&self) -> Result<String, CloudJoinError> {
        let (_, _, state) =
            cloud_bootstrap_inputs().map_err(|_| CloudJoinError::LocalSettingsChanged)?;
        let id = state
            .cloud_library_id
            .as_deref()
            .ok_or(CloudJoinError::NotConnected)?;
        let code = CloudJoinCode::new(&state.cloud_settings, id)?;
        inspect_target(&state.cloud_settings, id).await?;
        Ok(code.encode()?)
    }

    pub async fn preview_cloud_join_code(
        &self,
        text: &str,
    ) -> Result<CloudJoinPreview, CloudJoinError> {
        let code = CloudJoinCode::decode(text)?;
        let settings = code.settings(&CloudSettings::default());
        let (_, library) = inspect_target(&settings, &code.library_id).await?;
        Ok(CloudJoinPreview {
            backend: match code.backend {
                Backend::S3 { .. } => "S3",
                _ => "WebDAV",
            }
            .into(),
            endpoint: code.endpoint()?.into(),
            bucket: match &code.backend {
                Backend::S3 { bucket, .. } => Some(bucket.clone()),
                _ => None,
            },
            root_path: code.root_path,
            library_id: code.library_id,
            game_count: library.games.len(),
        })
    }

    pub async fn import_cloud_join_code(
        &self,
        text: &str,
        confirmed: bool,
    ) -> Result<CloudLibraryStatus, CloudJoinError> {
        if !confirmed {
            return Err(CloudJoinError::ConfirmationRequired);
        }
        let code = CloudJoinCode::decode(text)?;
        self.connect_join_target(&code.backend, &code.root_path, &code.library_id)
            .await
    }

    async fn connect_join_target(
        &self,
        backend: &Backend,
        root_path: &str,
        library_id: &str,
    ) -> Result<CloudLibraryStatus, CloudJoinError> {
        let (library, profile, state) =
            cloud_bootstrap_inputs().map_err(|_| CloudJoinError::LocalSettingsChanged)?;
        let settings = CloudSettings {
            backend: backend.clone(),
            root_path: root_path.into(),
            ..state.cloud_settings.clone()
        };
        let (operator, remote) = inspect_target(&settings, library_id).await?;
        let published = connected_cloud_profile(&library, &profile, &state, &remote)
            .map_err(|_| CloudJoinError::LocalSettingsChanged)?;
        DeviceProfileRepository::new(operator, 3)
            .publish(&state.current_device_id, &published)
            .await
            .map_err(|_| CloudJoinError::RegistrationFailed)?;
        connect_cloud_library_local(&library, &profile, &state, &remote, library_id, &settings)
            .map_err(|_| CloudJoinError::LocalSettingsChanged)?;
        Ok(CloudLibraryStatus::Active {
            game_count: remote.games.len(),
        })
    }
}

// Read-only verification shared by export, preview, and confirmation. In
// particular, an empty/replaced target must never turn an import into creation.
async fn inspect_target(
    settings: &CloudSettings,
    id: &str,
) -> Result<(opendal::Operator, SharedLibrary), CloudJoinError> {
    let operator = CloudSyncSessionConfig::from(settings)
        .get_op()
        .map_err(|_| CloudJoinError::Unavailable)?;
    match CloudLibraryBootstrap::new(operator.clone(), 3)
        .inspect()
        .await
        .map_err(|_| CloudJoinError::Unavailable)?
    {
        CloudNamespaceClassification::SupportedV2 {
            shared_library,
            descriptor,
            ..
        } if descriptor.library_id == id => Ok((operator, shared_library)),
        _ => Err(CloudJoinError::InvalidLibrary),
    }
}

#[cfg(test)]
#[path = "cloud_join_code_tests.rs"]
mod tests;
