use std::collections::BTreeMap;

use opendal::{ErrorKind, Operator};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::DELETION_REGISTRY_PATH;
use crate::device::DeviceId;

pub const DELETION_REGISTRY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletionRegistry {
    pub schema_version: u32,
    pub revision: u64,
    pub deleted_profiles: BTreeMap<DeviceId, ProfileDeletion>,
    pub deleted_games: BTreeMap<String, GameDeletion>,
}

impl Default for DeletionRegistry {
    fn default() -> Self {
        Self {
            schema_version: DELETION_REGISTRY_SCHEMA_VERSION,
            revision: 0,
            deleted_profiles: BTreeMap::new(),
            deleted_games: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileDeletion {
    pub deleted_by: DeviceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameDeletion {
    pub deleted_by: DeviceId,
    #[serde(default)]
    pub name: String,
}

pub struct DeletionRegistryRepository {
    operator: Operator,
    max_attempts: usize,
}

impl DeletionRegistryRepository {
    pub fn new(operator: Operator, max_attempts: usize) -> Self {
        Self {
            operator,
            max_attempts: max_attempts.max(1),
        }
    }

    pub async fn load(&self) -> Result<DeletionRegistry, DeletionRegistryError> {
        let bytes = match self.operator.read(DELETION_REGISTRY_PATH).await {
            Ok(bytes) => bytes.to_vec(),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(DeletionRegistry::default());
            }
            Err(error) => return Err(error.into()),
        };
        let registry: DeletionRegistry = serde_json::from_slice(&bytes)?;
        if registry.schema_version != DELETION_REGISTRY_SCHEMA_VERSION {
            return Err(DeletionRegistryError::UnsupportedSchema(
                registry.schema_version,
            ));
        }
        Ok(registry)
    }

    pub async fn ensure_active(
        &self,
        device_id: &str,
        game_id: &str,
    ) -> Result<(), DeletionRegistryError> {
        let registry = self.load().await?;
        if registry.deleted_profiles.contains_key(device_id) {
            return Err(DeletionRegistryError::ProfileDeleted(device_id.to_string()));
        }
        if registry.deleted_games.contains_key(game_id) {
            return Err(DeletionRegistryError::GameDeleted(game_id.to_string()));
        }
        Ok(())
    }

    pub async fn mark_profile_deleted(
        &self,
        device_id: &str,
        acting_device: &str,
    ) -> Result<DeletionRegistry, DeletionRegistryError> {
        self.mutate(|registry| {
            registry
                .deleted_profiles
                .entry(device_id.to_string())
                .or_insert_with(|| ProfileDeletion {
                    deleted_by: acting_device.to_string(),
                });
        })
        .await
    }

    pub async fn mark_game_deleted(
        &self,
        game_id: &str,
        game_name: &str,
        acting_device: &str,
    ) -> Result<DeletionRegistry, DeletionRegistryError> {
        self.mutate(|registry| {
            registry
                .deleted_games
                .entry(game_id.to_string())
                .or_insert_with(|| GameDeletion {
                    deleted_by: acting_device.to_string(),
                    name: game_name.to_string(),
                });
        })
        .await
    }

    async fn mutate(
        &self,
        mut change: impl FnMut(&mut DeletionRegistry),
    ) -> Result<DeletionRegistry, DeletionRegistryError> {
        for _ in 0..self.max_attempts {
            let mut accepted = self.load().await?;
            let previous = accepted.clone();
            change(&mut accepted);
            if accepted == previous {
                return Ok(accepted);
            }
            accepted.revision = accepted.revision.saturating_add(1);
            let bytes = serde_json::to_vec_pretty(&accepted)?;
            self.operator.write(DELETION_REGISTRY_PATH, bytes).await?;
            if self.load().await? == accepted {
                return Ok(accepted);
            }
        }
        Err(DeletionRegistryError::RetryExhausted {
            attempts: self.max_attempts,
        })
    }
}

#[derive(Debug, Error)]
pub enum DeletionRegistryError {
    #[error("Unsupported deletion registry schema: {0}")]
    UnsupportedSchema(u32),
    #[error("Deletion registry update was not visible after {attempts} attempts")]
    RetryExhausted { attempts: usize },
    #[error("Device Profile has been permanently removed: {0}")]
    ProfileDeleted(DeviceId),
    #[error("Shared Game has been permanently deleted: {0}")]
    GameDeleted(String),
    #[error("Deletion registry serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Deletion registry transport failed: {0}")]
    Transport(#[from] opendal::Error),
}

#[cfg(test)]
mod tests {
    use opendal::services;

    use super::*;

    #[tokio::test]
    async fn missing_registry_is_empty_and_markers_are_durable() {
        let operator = Operator::new(services::Memory::default()).unwrap().finish();
        let repository = DeletionRegistryRepository::new(operator.clone(), 2);

        assert_eq!(
            repository.load().await.unwrap(),
            DeletionRegistry::default()
        );
        repository.mark_profile_deleted("deck", "pc").await.unwrap();

        let stored = repository.load().await.unwrap();
        assert_eq!(stored.deleted_profiles["deck"].deleted_by, "pc");
        assert!(operator.exists(DELETION_REGISTRY_PATH).await.unwrap());
    }

    #[tokio::test]
    async fn existing_markers_are_idempotent_and_preserve_the_first_actor() {
        let operator = Operator::new(services::Memory::default()).unwrap().finish();
        let repository = DeletionRegistryRepository::new(operator, 2);

        repository.mark_profile_deleted("deck", "pc").await.unwrap();
        let stored = repository
            .mark_profile_deleted("deck", "laptop")
            .await
            .unwrap();

        assert_eq!(stored.deleted_profiles["deck"].deleted_by, "pc");
    }

    #[tokio::test]
    async fn game_marker_preserves_the_first_name_and_actor() {
        let operator = Operator::new(services::Memory::default()).unwrap().finish();
        let repository = DeletionRegistryRepository::new(operator, 2);

        repository
            .mark_game_deleted("game", "Original", "pc")
            .await
            .unwrap();
        let stored = repository
            .mark_game_deleted("game", "Stale", "deck")
            .await
            .unwrap();

        assert_eq!(stored.deleted_games["game"].name, "Original");
        assert_eq!(stored.deleted_games["game"].deleted_by, "pc");
    }

    #[tokio::test]
    async fn deleted_profile_and_game_are_rejected_as_writers() {
        let operator = Operator::new(services::Memory::default()).unwrap().finish();
        let repository = DeletionRegistryRepository::new(operator, 2);
        repository
            .mark_profile_deleted("removed-device", "pc")
            .await
            .unwrap();
        repository
            .mark_game_deleted("deleted-game", "Deleted Game", "pc")
            .await
            .unwrap();

        assert!(matches!(
            repository
                .ensure_active("removed-device", "active-game")
                .await,
            Err(DeletionRegistryError::ProfileDeleted(device)) if device == "removed-device"
        ));
        assert!(matches!(
            repository.ensure_active("pc", "deleted-game").await,
            Err(DeletionRegistryError::GameDeleted(game)) if game == "deleted-game"
        ));
    }
}
