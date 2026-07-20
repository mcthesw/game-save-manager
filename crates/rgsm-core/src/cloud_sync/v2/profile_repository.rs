use futures_util::TryStreamExt;
use opendal::Operator;
use thiserror::Error;

use super::{
    DeletionRegistryError, DeletionRegistryRepository, V2_DEVICE_PROFILES_PREFIX,
    device_profile_path,
};
use crate::config::{DeviceProfile, V2_CONFIG_SCHEMA_VERSION};
use crate::device::DeviceId;

pub struct DeviceProfileRepository {
    operator: Operator,
    max_attempts: usize,
}

impl DeviceProfileRepository {
    pub fn new(operator: Operator, max_attempts: usize) -> Self {
        Self {
            operator,
            max_attempts: max_attempts.max(1),
        }
    }

    /// Publish the current Device's complete Profile and verify provider-visible bytes.
    ///
    /// Only the Device named by `profile.device.id` may call this operation.
    /// Provider locks are intentionally outside the V2 contract.
    pub async fn publish(
        &self,
        acting_device_id: &str,
        profile: &DeviceProfile,
    ) -> Result<(), DeviceProfileRepositoryError> {
        let registry = DeletionRegistryRepository::new(self.operator.clone(), self.max_attempts)
            .load()
            .await?;
        if registry.deleted_profiles.contains_key(acting_device_id) {
            return Err(DeviceProfileRepositoryError::Deleted(
                acting_device_id.to_string(),
            ));
        }
        if let Some(game_id) = profile
            .games
            .keys()
            .find(|game_id| registry.deleted_games.contains_key(*game_id))
        {
            return Err(DeviceProfileRepositoryError::DeletedGame(game_id.clone()));
        }
        if profile.schema_version != V2_CONFIG_SCHEMA_VERSION {
            return Err(DeviceProfileRepositoryError::UnsupportedSchema(
                profile.schema_version,
            ));
        }
        if profile.device.id != acting_device_id {
            return Err(DeviceProfileRepositoryError::WrongDevice {
                acting: acting_device_id.to_string(),
                profile: profile.device.id.clone(),
            });
        }
        let path = device_profile_path(acting_device_id);
        let expected = serde_json::to_vec_pretty(profile)?;
        for _ in 0..self.max_attempts {
            self.operator.write(&path, expected.clone()).await?;
            if self
                .operator
                .read(&path)
                .await
                .is_ok_and(|stored| stored.to_vec() == expected)
            {
                return Ok(());
            }
        }
        Err(DeviceProfileRepositoryError::RetryExhausted {
            attempts: self.max_attempts,
        })
    }

    pub async fn list(&self) -> Result<Vec<DeviceProfile>, DeviceProfileRepositoryError> {
        let mut lister = self.operator.lister(V2_DEVICE_PROFILES_PREFIX).await?;
        let mut profiles = Vec::new();
        while let Some(entry) = lister.try_next().await? {
            let path = entry.path();
            if !path.ends_with(".json") {
                continue;
            }
            let bytes = self.operator.read(path).await?;
            let profile: DeviceProfile = serde_json::from_slice(&bytes.to_vec())?;
            if profile.schema_version != V2_CONFIG_SCHEMA_VERSION {
                return Err(DeviceProfileRepositoryError::UnsupportedSchema(
                    profile.schema_version,
                ));
            }
            if device_profile_path(&profile.device.id) != path {
                return Err(DeviceProfileRepositoryError::PathIdentityMismatch {
                    path: path.to_string(),
                    device_id: profile.device.id,
                });
            }
            profiles.push(profile);
        }
        profiles.sort_by(|left, right| left.device.id.cmp(&right.device.id));
        Ok(profiles)
    }

    pub async fn delete(&self, device_id: &str) -> Result<(), DeviceProfileRepositoryError> {
        let path = device_profile_path(device_id);
        for _ in 0..self.max_attempts {
            self.operator.delete(&path).await?;
            if !self.operator.exists(&path).await? {
                return Ok(());
            }
        }
        Err(DeviceProfileRepositoryError::DeleteRetryExhausted {
            device_id: device_id.to_string(),
            attempts: self.max_attempts,
        })
    }

    /// Remove every durably deleted Game from all live Device Profiles.
    ///
    /// Cleaning all known markers together keeps overlapping Game deletions
    /// independently retryable without temporarily republishing another
    /// deleted Game through a whole-Profile write.
    pub async fn remove_deleted_game_state(&self) -> Result<usize, DeviceProfileRepositoryError> {
        let registry = DeletionRegistryRepository::new(self.operator.clone(), self.max_attempts)
            .load()
            .await?;
        let mut changed = 0;
        for mut profile in self.list().await? {
            if registry.deleted_profiles.contains_key(&profile.device.id) {
                continue;
            }
            let mut removed = false;
            for (game_id, deletion) in &registry.deleted_games {
                removed |= profile.remove_game_state(game_id, &deletion.name);
            }
            if removed {
                let device_id = profile.device.id.clone();
                self.publish(&device_id, &profile).await?;
                changed += 1;
            }
        }
        Ok(changed)
    }
}

#[derive(Debug, Error)]
pub enum DeviceProfileRepositoryError {
    #[error("Unsupported Device Profile schema: {0}")]
    UnsupportedSchema(u32),
    #[error("Device {acting} cannot publish Device Profile {profile}")]
    WrongDevice { acting: String, profile: String },
    #[error("Device Profile {0} has been permanently removed")]
    Deleted(DeviceId),
    #[error("Game {0} has been permanently deleted")]
    DeletedGame(String),
    #[error("Device Profile path {path} does not match embedded Device ID {device_id}")]
    PathIdentityMismatch { path: String, device_id: DeviceId },
    #[error("Device Profile read-back verification failed after {attempts} attempts")]
    RetryExhausted { attempts: usize },
    #[error("Device Profile {device_id} remained visible after {attempts} delete attempts")]
    DeleteRetryExhausted {
        device_id: DeviceId,
        attempts: usize,
    },
    #[error("Device Profile serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Device Profile transport failed: {0}")]
    Transport(#[from] opendal::Error),
    #[error(transparent)]
    DeletionRegistry(#[from] DeletionRegistryError),
}

#[cfg(test)]
mod tests {
    use opendal::services;

    use super::*;
    use crate::backup::CompressionPreset;
    use crate::config::{DeviceBehaviorSettings, QuickActionsSettings};
    use crate::device::Device;

    fn profile() -> DeviceProfile {
        DeviceProfile {
            schema_version: V2_CONFIG_SCHEMA_VERSION,
            device: Device {
                id: "deck".into(),
                name: "Steam Deck".into(),
                resources: Vec::new(),
                next_resource_id: 0,
            },
            local_archive_root: Some("save_data".into()),
            games: Default::default(),
            private_favorites: Vec::new(),
            quick_action: QuickActionsSettings::default(),
            behavior: DeviceBehaviorSettings {
                prompt_when_not_described: false,
                extra_backup_when_apply: true,
                confirm_before_apply_latest: true,
                confirm_before_apply_snapshot: true,
                prompt_when_auto_backup: false,
                default_delete_before_apply: false,
                add_new_to_favorites: false,
                vn_scan_dirs: Vec::new(),
                max_auto_backup_count: 0,
                max_extra_backup_count: 0,
                compression_preset: CompressionPreset::default(),
                compute_archive_hash: true,
                verify_archive_before_apply: true,
            },
        }
    }

    #[tokio::test]
    async fn only_the_current_device_can_publish_its_verified_profile() {
        let operator = Operator::new(services::Memory::default()).unwrap().finish();
        let repository = DeviceProfileRepository::new(operator.clone(), 2);

        assert!(matches!(
            repository.publish("pc", &profile()).await,
            Err(DeviceProfileRepositoryError::WrongDevice { .. })
        ));
        repository.publish("deck", &profile()).await.unwrap();
        assert!(operator.read(&device_profile_path("deck")).await.is_ok());
        assert_eq!(
            repository.list().await.unwrap()[0].device.name,
            "Steam Deck"
        );
    }
}
