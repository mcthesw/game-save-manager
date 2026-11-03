use std::collections::BTreeMap;

use opendal::{ErrorKind, Operator};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::metadata_read::read_complete_json;
use super::read_round::{cached_read, invalidate};
use super::{CLOUD_ARCHIVES_PREFIX, DELETION_REGISTRY_PATH};
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

    /// Read the registry, reusing the answer already fetched in this
    /// reconciliation round. Guards that run right before a write use
    /// [`Self::load_fresh`].
    pub async fn load(&self) -> Result<DeletionRegistry, DeletionRegistryError> {
        let bytes = cached_read(Some(DELETION_REGISTRY_PATH), || self.read_complete()).await?;
        Self::decode(bytes)
    }

    pub async fn load_fresh(&self) -> Result<DeletionRegistry, DeletionRegistryError> {
        Self::decode(self.read_complete().await?)
    }

    async fn read_complete(&self) -> Result<Option<Vec<u8>>, opendal::Error> {
        read_complete_json(
            || async {
                match self.operator.read(DELETION_REGISTRY_PATH).await {
                    Ok(bytes) => Ok(Some(bytes.to_vec())),
                    Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
                    Err(error) => Err(error),
                }
            },
            self.max_attempts,
        )
        .await
    }

    fn decode(bytes: Option<Vec<u8>>) -> Result<DeletionRegistry, DeletionRegistryError> {
        // Absence must not read as "nothing was ever deleted": that would let
        // permanently deleted Games and removed Devices come back.
        let Some(bytes) = bytes else {
            return Err(DeletionRegistryError::Missing);
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
        Self::check_active(&self.load().await?, device_id, game_id)
    }

    pub async fn ensure_active_fresh(
        &self,
        device_id: &str,
        game_id: &str,
    ) -> Result<(), DeletionRegistryError> {
        Self::check_active(&self.load_fresh().await?, device_id, game_id)
    }

    fn check_active(
        registry: &DeletionRegistry,
        device_id: &str,
        game_id: &str,
    ) -> Result<(), DeletionRegistryError> {
        if registry.deleted_profiles.contains_key(device_id) {
            return Err(DeletionRegistryError::ProfileDeleted(device_id.to_string()));
        }
        if registry.deleted_games.contains_key(game_id) {
            return Err(DeletionRegistryError::GameDeleted(game_id.to_string()));
        }
        Ok(())
    }

    /// Verify that a Device can write a Game and converge stale Archives when
    /// its durable deletion marker already exists.
    pub async fn ensure_active_and_converge_game_archives(
        &self,
        device_id: &str,
        game_id: &str,
    ) -> Result<(), DeletionRegistryError> {
        let registry = self.load().await?;
        self.converge_if_deleted(&registry, device_id, game_id)
            .await
    }

    /// Same check for the guard right after a write, which must see a deletion
    /// that raced with it rather than a cached answer.
    pub async fn ensure_active_and_converge_game_archives_fresh(
        &self,
        device_id: &str,
        game_id: &str,
    ) -> Result<(), DeletionRegistryError> {
        let registry = self.load_fresh().await?;
        self.converge_if_deleted(&registry, device_id, game_id)
            .await
    }

    async fn converge_if_deleted(
        &self,
        registry: &DeletionRegistry,
        device_id: &str,
        game_id: &str,
    ) -> Result<(), DeletionRegistryError> {
        if registry.deleted_profiles.contains_key(device_id) {
            return Err(DeletionRegistryError::ProfileDeleted(device_id.to_string()));
        }
        if !registry.deleted_games.contains_key(game_id) {
            return Ok(());
        }
        self.operator
            .delete_with(&format!("{CLOUD_ARCHIVES_PREFIX}{game_id}/"))
            .recursive(true)
            .await?;
        Err(DeletionRegistryError::GameDeleted(game_id.to_string()))
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

    pub(super) async fn reactivate_profile(
        &self,
        device_id: &str,
    ) -> Result<(), DeletionRegistryError> {
        self.mutate(|registry| {
            registry.deleted_profiles.remove(device_id);
        })
        .await?;
        Ok(())
    }

    async fn mutate(
        &self,
        mut change: impl FnMut(&mut DeletionRegistry),
    ) -> Result<DeletionRegistry, DeletionRegistryError> {
        for _ in 0..self.max_attempts {
            let mut accepted = self.load_fresh().await?;
            let previous = accepted.clone();
            change(&mut accepted);
            if accepted == previous {
                return Ok(accepted);
            }
            accepted.revision = accepted.revision.saturating_add(1);
            let bytes = serde_json::to_vec_pretty(&accepted)?;
            let written = self.operator.write(DELETION_REGISTRY_PATH, bytes).await;
            invalidate(DELETION_REGISTRY_PATH);
            written?;
            if self.load_fresh().await? == accepted {
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
    #[error("{}", rust_i18n::t!("error.cloud_library_damaged"))]
    Missing,
    #[error("Unsupported deletion registry schema: {0}")]
    UnsupportedSchema(u32),
    #[error("Deletion registry update was not visible after {attempts} attempts")]
    RetryExhausted { attempts: usize },
    #[error("Device {0} was removed; confirm reconnection on that device to continue")]
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
    async fn missing_registry_is_reported_and_never_recreated_by_a_marker() {
        let operator = Operator::new(services::Memory::default()).unwrap().finish();
        let repository = DeletionRegistryRepository::new(operator.clone(), 2);

        assert!(matches!(
            repository.load().await,
            Err(DeletionRegistryError::Missing)
        ));
        assert!(matches!(
            repository.mark_profile_deleted("deck", "pc").await,
            Err(DeletionRegistryError::Missing)
        ));
        assert!(!operator.exists(DELETION_REGISTRY_PATH).await.unwrap());
    }

    #[tokio::test]
    async fn markers_are_durable() {
        let operator = crate::cloud_sync::v2::test_support::seeded_memory_operator().await;
        let repository = DeletionRegistryRepository::new(operator.clone(), 2);
        repository.mark_profile_deleted("deck", "pc").await.unwrap();

        let stored = repository.load().await.unwrap();
        assert_eq!(stored.deleted_profiles["deck"].deleted_by, "pc");
        assert!(operator.exists(DELETION_REGISTRY_PATH).await.unwrap());
    }

    #[tokio::test]
    async fn deletion_registry_read_waits_for_an_in_progress_overwrite() {
        let root = temp_dir::TempDir::new().unwrap();
        let operator =
            Operator::new(services::Fs::default().root(root.path().to_string_lossy().as_ref()))
                .unwrap()
                .finish();
        operator
            .write(DELETION_REGISTRY_PATH, Vec::<u8>::new())
            .await
            .unwrap();
        let writer = operator.clone();
        let write = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let registry = DeletionRegistry {
                revision: 7,
                ..DeletionRegistry::default()
            };
            writer
                .write(
                    DELETION_REGISTRY_PATH,
                    serde_json::to_vec(&registry).unwrap(),
                )
                .await
                .unwrap();
        });

        let result = DeletionRegistryRepository::new(operator, 10).load().await;
        write.await.unwrap();
        assert_eq!(result.unwrap().revision, 7);
    }

    #[tokio::test]
    async fn existing_markers_are_idempotent_and_preserve_the_first_actor() {
        let operator = crate::cloud_sync::v2::test_support::seeded_memory_operator().await;
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
        let operator = crate::cloud_sync::v2::test_support::seeded_memory_operator().await;
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
        let operator = crate::cloud_sync::v2::test_support::seeded_memory_operator().await;
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
