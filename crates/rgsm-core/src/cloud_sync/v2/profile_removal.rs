use opendal::Operator;
use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use super::{
    CLOUD_MANIFEST_PATH, CloudManifestRepository, DeletionRegistryError,
    DeletionRegistryRepository, DeviceProfileRepository, DeviceProfileRepositoryError,
    ManifestRepositoryError,
};
use crate::device::DeviceId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct DeviceProfileRemovalOutcome {
    pub device_id: DeviceId,
    pub removed_heads: usize,
}

pub struct DeviceProfileRemoval {
    operator: Operator,
    current_device_id: DeviceId,
    max_attempts: usize,
}

impl DeviceProfileRemoval {
    pub fn new(operator: Operator, current_device_id: DeviceId, max_attempts: usize) -> Self {
        Self {
            operator,
            current_device_id,
            max_attempts: max_attempts.max(1),
        }
    }

    /// Publish a durable Profile Tombstone before removing the Profile object
    /// and all of that Device's Heads in the same retryable operation.
    pub async fn remove(
        &self,
        device_id: &str,
        confirmed: bool,
    ) -> Result<DeviceProfileRemovalOutcome, DeviceProfileRemovalError> {
        if !confirmed {
            return Err(DeviceProfileRemovalError::ConfirmationRequired);
        }
        if device_id == self.current_device_id {
            return Err(DeviceProfileRemovalError::CurrentDevice);
        }
        DeletionRegistryRepository::new(self.operator.clone(), self.max_attempts)
            .mark_profile_deleted(device_id, &self.current_device_id)
            .await?;
        DeviceProfileRepository::new(self.operator.clone(), self.max_attempts)
            .delete(device_id)
            .await?;

        let device_id_owned = device_id.to_string();
        let manifest_repository = CloudManifestRepository::new(
            self.operator.clone(),
            CLOUD_MANIFEST_PATH,
            self.max_attempts,
        );
        let before = manifest_repository.load().await?;
        let removed_heads = before
            .games
            .values()
            .filter(|game| game.device_heads.contains_key(device_id))
            .count();
        manifest_repository
            .mutate(move |manifest| {
                for game in manifest.games.values_mut() {
                    game.device_heads.remove(&device_id_owned);
                }
                Ok(())
            })
            .await?;
        let verified = manifest_repository.load().await?;
        if verified
            .games
            .values()
            .any(|game| game.device_heads.contains_key(device_id))
        {
            return Err(DeviceProfileRemovalError::HeadsRemain(
                device_id.to_string(),
            ));
        }
        Ok(DeviceProfileRemovalOutcome {
            device_id: device_id.to_string(),
            removed_heads,
        })
    }
}

#[derive(Debug, Error)]
pub enum DeviceProfileRemovalError {
    #[error("Removing a Device Profile requires explicit confirmation")]
    ConfirmationRequired,
    #[error("The current Device Profile cannot remove itself")]
    CurrentDevice,
    #[error("Device {0} still owns one or more Heads after removal")]
    HeadsRemain(DeviceId),
    #[error(transparent)]
    Registry(#[from] DeletionRegistryError),
    #[error(transparent)]
    Profile(#[from] DeviceProfileRepositoryError),
    #[error(transparent)]
    Manifest(#[from] ManifestRepositoryError),
}

#[cfg(test)]
mod tests {
    use opendal::services;

    use super::*;
    use crate::backup::{ArchiveFormat, CreatedBy};
    use crate::cloud_sync::v2::{
        ArchiveIntegrity, CloudManifest, GameManifest, SnapshotNode, device_profile_path,
    };
    use crate::config::ConfigurationOwners;

    #[tokio::test]
    async fn tombstone_blocks_stale_profile_and_removes_only_matching_heads() {
        let operator = Operator::new(services::Memory::default()).unwrap().finish();
        let owners = ConfigurationOwners::from_legacy(
            &crate::config::Config::default(),
            &"deck".to_string(),
        );
        let deck = owners.device_profiles["deck"].clone();
        DeviceProfileRepository::new(operator.clone(), 2)
            .publish("deck", &deck)
            .await
            .unwrap();
        let mut manifest = CloudManifest::default();
        let mut game = GameManifest::new("game");
        game.upsert_live(SnapshotNode::live(
            "snapshot",
            None,
            ArchiveIntegrity {
                size: 1,
                xxh3_64: "0000000000000001".into(),
            },
            CreatedBy::Manual,
        ))
        .unwrap();
        game.snapshots.get_mut("snapshot").unwrap().archive_format = ArchiveFormat::Zip;
        game.set_head("pc".into(), "snapshot".into());
        game.set_head("deck".into(), "snapshot".into());
        manifest.games.insert("game".into(), game);
        operator
            .write(
                CLOUD_MANIFEST_PATH,
                serde_json::to_vec_pretty(&manifest).unwrap(),
            )
            .await
            .unwrap();

        let outcome = DeviceProfileRemoval::new(operator.clone(), "pc".into(), 2)
            .remove("deck", true)
            .await
            .unwrap();

        assert_eq!(outcome.removed_heads, 1);
        assert!(!operator.exists(&device_profile_path("deck")).await.unwrap());
        let stored = CloudManifestRepository::new(operator.clone(), CLOUD_MANIFEST_PATH, 2)
            .load()
            .await
            .unwrap();
        assert_eq!(stored.games["game"].device_heads.len(), 1);
        assert!(stored.games["game"].device_heads.contains_key("pc"));
        assert!(stored.games["game"].snapshots["snapshot"].state.is_live());
        assert!(matches!(
            DeviceProfileRepository::new(operator, 2)
                .publish("deck", &deck)
                .await,
            Err(DeviceProfileRepositoryError::Deleted(device)) if device == "deck"
        ));
    }
}
