use opendal::Operator;
use thiserror::Error;

use super::device_profile_path;
use crate::config::{DeviceProfile, V2_CONFIG_SCHEMA_VERSION};

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
}

#[derive(Debug, Error)]
pub enum DeviceProfileRepositoryError {
    #[error("Unsupported Device Profile schema: {0}")]
    UnsupportedSchema(u32),
    #[error("Device {acting} cannot publish Device Profile {profile}")]
    WrongDevice { acting: String, profile: String },
    #[error("Device Profile read-back verification failed after {attempts} attempts")]
    RetryExhausted { attempts: usize },
    #[error("Device Profile serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Device Profile transport failed: {0}")]
    Transport(#[from] opendal::Error),
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
    }
}
