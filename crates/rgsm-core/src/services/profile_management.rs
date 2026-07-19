use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::cloud_sync::CloudSyncSessionConfig;
use crate::cloud_sync::v2::{
    CLOUD_MANIFEST_PATH, CloudManifestRepository, DeletionRegistryRepository, DeviceProfileRemoval,
    DeviceProfileRemovalOutcome, DeviceProfileRepository,
};
use crate::config::{CloudNamespaceGeneration, cloud_bootstrap_inputs, remove_device_profile};

use super::{CloudLibraryServiceError, ServiceContext};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CloudDeviceProfileView {
    pub device_id: String,
    pub name: String,
    pub current: bool,
    pub deleted: bool,
    pub deletion_incomplete: bool,
    pub head_count: usize,
}

impl ServiceContext {
    pub async fn cloud_device_profiles(
        &self,
    ) -> Result<Vec<CloudDeviceProfileView>, CloudLibraryServiceError> {
        let (_, _, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let session = CloudSyncSessionConfig::from(&local_state.cloud_settings);
        let operator = session.get_op()?;
        let profiles = DeviceProfileRepository::new(operator.clone(), 3)
            .list()
            .await?
            .into_iter()
            .map(|profile| (profile.device.id.clone(), profile))
            .collect::<BTreeMap<_, _>>();
        let registry = DeletionRegistryRepository::new(operator.clone(), 3)
            .load()
            .await?;
        let manifest = CloudManifestRepository::new(operator, CLOUD_MANIFEST_PATH, 3)
            .load()
            .await?;
        let device_ids = profiles
            .keys()
            .chain(registry.deleted_profiles.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        Ok(device_ids
            .into_iter()
            .map(|device_id| {
                let profile = profiles.get(&device_id);
                let deleted = registry.deleted_profiles.contains_key(&device_id);
                let head_count = manifest
                    .games
                    .values()
                    .filter(|game| game.device_heads.contains_key(&device_id))
                    .count();
                CloudDeviceProfileView {
                    name: profile
                        .map(|profile| profile.device.name.clone())
                        .unwrap_or_else(|| device_id.clone()),
                    current: device_id == local_state.current_device_id,
                    deletion_incomplete: deleted && (profile.is_some() || head_count > 0),
                    device_id,
                    deleted,
                    head_count,
                }
            })
            .collect())
    }

    pub async fn remove_cloud_device_profile(
        &self,
        device_id: &str,
        confirmed: bool,
    ) -> Result<DeviceProfileRemovalOutcome, CloudLibraryServiceError> {
        let (_, _, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let session = CloudSyncSessionConfig::from(&local_state.cloud_settings);
        let outcome =
            DeviceProfileRemoval::new(session.get_op()?, local_state.current_device_id, 3)
                .remove(device_id, confirmed)
                .await?;
        remove_device_profile(device_id)?;
        Ok(outcome)
    }
}
