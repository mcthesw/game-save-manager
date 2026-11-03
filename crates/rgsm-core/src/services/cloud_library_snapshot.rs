use serde::Serialize;
use specta::Type;

use crate::cloud_sync::v2::CloudArchiveLibraryView;

use super::{
    CloudLibraryServiceError, DeletedCloudGameView, ServiceContext,
    profile_management::CloudDeviceProfileView,
};

/// Everything the sync pages show about the cloud library. One reconciliation
/// round produces it, so opening a page never needs its own cloud requests.
#[derive(Debug, Clone, Serialize, Type, utoipa::ToSchema)]
pub struct CloudLibrarySnapshot {
    pub library: CloudArchiveLibraryView,
    pub deleted_games: Vec<DeletedCloudGameView>,
    pub device_profiles: Vec<CloudDeviceProfileView>,
}

impl ServiceContext {
    /// Call inside the reconciliation round whose reads it should share.
    pub async fn cloud_library_snapshot(
        &self,
    ) -> Result<CloudLibrarySnapshot, CloudLibraryServiceError> {
        Ok(CloudLibrarySnapshot {
            library: self.cloud_archive_library().await?,
            deleted_games: self.deleted_cloud_games().await?,
            device_profiles: self.cloud_device_profiles().await?,
        })
    }
}
