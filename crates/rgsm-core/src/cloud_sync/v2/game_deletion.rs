use std::path::{Component, Path, PathBuf};

use futures_util::TryStreamExt;
use opendal::Operator;
use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use super::{
    CLOUD_ARCHIVES_PREFIX, CLOUD_MANIFEST_PATH, CloudManifestRepository, DeletionRegistryError,
    DeletionRegistryRepository, DeviceProfileRepository, DeviceProfileRepositoryError,
    ManifestRepositoryError, SharedLibraryRepository, SharedLibraryRepositoryError,
};
use crate::device::DeviceId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct SharedGameDeletionOutcome {
    pub game_id: String,
    pub removed_snapshots: usize,
    pub cleaned_profiles: usize,
    pub removed_cloud_objects: usize,
}

pub struct SharedGameDeletion {
    operator: Operator,
    local_archive_root: PathBuf,
    current_device_id: DeviceId,
    max_attempts: usize,
}

impl SharedGameDeletion {
    pub fn new(
        operator: Operator,
        local_archive_root: PathBuf,
        current_device_id: DeviceId,
        max_attempts: usize,
    ) -> Self {
        Self {
            operator,
            local_archive_root,
            current_device_id,
            max_attempts: max_attempts.max(1),
        }
    }

    /// Permanently remove one shared Game after its global marker is durable.
    ///
    /// Every step is idempotent so an interrupted operation can be retried
    /// directly. The marker is never removed and therefore always dominates
    /// stale Shared Library, Profile, Manifest, and Archive writes.
    pub async fn delete(
        &self,
        game_id: &str,
        game_name: &str,
        confirmed: bool,
    ) -> Result<SharedGameDeletionOutcome, SharedGameDeletionError> {
        if !confirmed {
            return Err(SharedGameDeletionError::ConfirmationRequired);
        }
        validate_game_id(game_id)?;
        let registry = DeletionRegistryRepository::new(self.operator.clone(), self.max_attempts);
        registry
            .mark_game_deleted(game_id, game_name, &self.current_device_id)
            .await?;

        let manifest = CloudManifestRepository::new(
            self.operator.clone(),
            CLOUD_MANIFEST_PATH,
            self.max_attempts,
        );
        let before = manifest.load().await?;
        let removed_snapshots = before
            .games
            .get(game_id)
            .map_or(0, |game| game.snapshots.len());
        let archive_prefix = cloud_game_archive_prefix(game_id)?;
        let removed_cloud_objects = self.list_cloud_objects(&archive_prefix).await?.len();
        remove_local_game_directory(&self.local_archive_root, game_id).await?;
        self.remove_cloud_archives(&archive_prefix).await?;

        let shared = SharedLibraryRepository::new(self.operator.clone(), self.max_attempts);
        let expected = shared.load().await?;
        if expected
            .games
            .iter()
            .any(|game| game.storage_key == game_id)
        {
            let mut accepted = expected.clone();
            accepted.games.retain(|game| game.storage_key != game_id);
            shared.compare_replace(&expected, &accepted).await?;
        }

        let profiles = DeviceProfileRepository::new(self.operator.clone(), self.max_attempts);
        let cleaned_profiles = profiles.remove_deleted_game_state().await?;

        let deleted_game_ids = registry
            .load()
            .await?
            .deleted_games
            .into_keys()
            .collect::<Vec<_>>();
        if before
            .games
            .keys()
            .any(|game_id| deleted_game_ids.contains(game_id))
        {
            manifest
                .mutate(move |accepted| {
                    for deleted_game_id in &deleted_game_ids {
                        accepted.games.remove(deleted_game_id);
                    }
                    Ok(())
                })
                .await?;
        }

        self.verify(game_id, game_name, &archive_prefix).await?;

        Ok(SharedGameDeletionOutcome {
            game_id: game_id.to_string(),
            removed_snapshots,
            cleaned_profiles,
            removed_cloud_objects,
        })
    }

    async fn remove_cloud_archives(&self, prefix: &str) -> Result<(), SharedGameDeletionError> {
        for _ in 0..self.max_attempts {
            self.operator.delete_with(prefix).recursive(true).await?;
            if self.list_cloud_objects(prefix).await?.is_empty() {
                return Ok(());
            }
        }
        Err(SharedGameDeletionError::CloudArchivesRemain(
            prefix.to_string(),
        ))
    }

    async fn list_cloud_objects(
        &self,
        prefix: &str,
    ) -> Result<Vec<String>, SharedGameDeletionError> {
        let mut lister = self.operator.lister(prefix).await?;
        let mut paths = Vec::new();
        while let Some(entry) = lister.try_next().await? {
            paths.push(entry.path().to_string());
        }
        Ok(paths)
    }

    async fn verify(
        &self,
        game_id: &str,
        game_name: &str,
        archive_prefix: &str,
    ) -> Result<(), SharedGameDeletionError> {
        let registry = DeletionRegistryRepository::new(self.operator.clone(), self.max_attempts)
            .load()
            .await?;
        if !registry.deleted_games.contains_key(game_id) {
            return Err(SharedGameDeletionError::MarkerMissing(game_id.to_string()));
        }
        if SharedLibraryRepository::new(self.operator.clone(), self.max_attempts)
            .load()
            .await?
            .games
            .iter()
            .any(|game| game.storage_key == game_id)
        {
            return Err(SharedGameDeletionError::SharedDefinitionRemains(
                game_id.to_string(),
            ));
        }
        if DeviceProfileRepository::new(self.operator.clone(), self.max_attempts)
            .list()
            .await?
            .into_iter()
            .filter(|profile| !registry.deleted_profiles.contains_key(&profile.device.id))
            .any(|profile| profile.references_game(game_id, game_name))
        {
            return Err(SharedGameDeletionError::ProfileStateRemains(
                game_id.to_string(),
            ));
        }
        if CloudManifestRepository::new(
            self.operator.clone(),
            CLOUD_MANIFEST_PATH,
            self.max_attempts,
        )
        .load()
        .await?
        .games
        .contains_key(game_id)
        {
            return Err(SharedGameDeletionError::ManifestStateRemains(
                game_id.to_string(),
            ));
        }
        if !self.list_cloud_objects(archive_prefix).await?.is_empty() {
            return Err(SharedGameDeletionError::CloudArchivesRemain(
                archive_prefix.to_string(),
            ));
        }
        if self.local_archive_root.join(game_id).exists() {
            return Err(SharedGameDeletionError::LocalArchivesRemain(
                game_id.to_string(),
            ));
        }
        Ok(())
    }
}

pub(crate) async fn remove_local_game_directory(
    local_archive_root: &Path,
    game_id: &str,
) -> Result<(), SharedGameDeletionError> {
    validate_game_id(game_id)?;
    let path = local_archive_root.join(game_id);
    match tokio::fs::remove_dir_all(&path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn cloud_game_archive_prefix(game_id: &str) -> Result<String, SharedGameDeletionError> {
    validate_game_id(game_id)?;
    Ok(format!("{CLOUD_ARCHIVES_PREFIX}{game_id}/"))
}

fn validate_game_id(game_id: &str) -> Result<(), SharedGameDeletionError> {
    let mut components = Path::new(game_id).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(SharedGameDeletionError::UnsafeGameId(game_id.to_string())),
    }
}

#[derive(Debug, Error)]
pub enum SharedGameDeletionError {
    #[error("Permanent shared Game deletion requires explicit confirmation")]
    ConfirmationRequired,
    #[error("Game ID is not a safe archive path segment: {0}")]
    UnsafeGameId(String),
    #[error("Permanent deletion marker is missing for Game {0}")]
    MarkerMissing(String),
    #[error("Shared Game definition remains after deletion: {0}")]
    SharedDefinitionRemains(String),
    #[error("Device Profile state remains after deleting Game {0}")]
    ProfileStateRemains(String),
    #[error("Cloud Manifest state remains after deleting Game {0}")]
    ManifestStateRemains(String),
    #[error("Cloud Archives remain under {0}")]
    CloudArchivesRemain(String),
    #[error("Local Archives remain for Game {0}")]
    LocalArchivesRemain(String),
    #[error(transparent)]
    Registry(#[from] DeletionRegistryError),
    #[error(transparent)]
    SharedLibrary(#[from] SharedLibraryRepositoryError),
    #[error(transparent)]
    Profile(#[from] DeviceProfileRepositoryError),
    #[error(transparent)]
    Manifest(#[from] ManifestRepositoryError),
    #[error("Local Archive deletion failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Cloud Archive deletion failed: {0}")]
    Transport(#[from] opendal::Error),
}

#[cfg(test)]
mod tests {
    use opendal::services;

    use super::*;
    use crate::backup::{ArchiveFormat, CreatedBy};
    use crate::cloud_sync::v2::{
        ArchiveIntegrity, CloudManifest, DeviceProfileRepositoryError, GameManifest,
        ManifestRepositoryError, SharedLibraryRepositoryError, SnapshotNode, cloud_archive_path,
        device_profile_path,
    };
    use crate::config::{
        ConfigurationOwners, DeviceGameProfile, InitialCatchUpPolicy, SharedGame, SharedLibrary,
        SyncMode, V2_CONFIG_SCHEMA_VERSION,
    };

    fn device_game() -> DeviceGameProfile {
        DeviceGameProfile {
            visible: true,
            cloud_sync_enabled: true,
            sync_mode: SyncMode::CloudBackup,
            snapshot_sync_activation_revision: Some(1),
            snapshot_sync_local_baseline: Default::default(),
            initial_catch_up: InitialCatchUpPolicy::KeepRemote,
            live_save_process_name: None,
            live_save_snapshot_on_exit: false,
            multi_device_sync_suspended: false,
            game_path: Some("/games/example".into()),
            binding: None,
            auto_backup: None,
            save_units: Default::default(),
        }
    }

    fn shared_library() -> SharedLibrary {
        SharedLibrary {
            schema_version: V2_CONFIG_SCHEMA_VERSION,
            games: vec![SharedGame {
                name: "Example".into(),
                storage_key: "game".into(),
                save_units: Vec::new(),
                next_save_unit_id: 0,
                ludusavi_meta: None,
                snapshot_retention: None,
            }],
        }
    }

    async fn fixture() -> (Operator, temp_dir::TempDir) {
        let operator = Operator::new(services::Memory::default()).unwrap().finish();
        operator
            .write(
                super::super::SHARED_LIBRARY_PATH,
                serde_json::to_vec_pretty(&shared_library()).unwrap(),
            )
            .await
            .unwrap();
        let mut owners =
            ConfigurationOwners::from_legacy(&crate::config::Config::default(), &"pc".to_string());
        let profile = owners.device_profiles.get_mut("pc").unwrap();
        profile.games.insert("game".into(), device_game());
        profile.quick_action.quick_action_game_id = Some("game".into());
        DeviceProfileRepository::new(operator.clone(), 2)
            .publish("pc", profile)
            .await
            .unwrap();

        let mut manifest = CloudManifest::default();
        let mut game = GameManifest::new("game");
        game.upsert_live(SnapshotNode::live(
            "snapshot",
            None,
            ArchiveIntegrity {
                size: 4,
                xxh3_64: "0000000000000001".into(),
            },
            CreatedBy::Manual,
        ))
        .unwrap();
        game.set_head("pc".into(), "snapshot".into());
        manifest.games.insert("game".into(), game);
        operator
            .write(
                CLOUD_MANIFEST_PATH,
                serde_json::to_vec_pretty(&manifest).unwrap(),
            )
            .await
            .unwrap();
        operator
            .write(
                &cloud_archive_path("game", "snapshot", ArchiveFormat::Zip).unwrap(),
                b"data".to_vec(),
            )
            .await
            .unwrap();

        let local = temp_dir::TempDir::new().unwrap();
        tokio::fs::create_dir_all(local.path().join("game"))
            .await
            .unwrap();
        tokio::fs::write(local.path().join("game").join("snapshot.zip"), b"data")
            .await
            .unwrap();
        (operator, local)
    }

    #[tokio::test]
    async fn marker_first_deletion_removes_every_game_owned_surface_and_is_retryable() {
        let (operator, local) = fixture().await;
        let deletion =
            SharedGameDeletion::new(operator.clone(), local.path().into(), "pc".into(), 2);

        let outcome = deletion.delete("game", "Example", true).await.unwrap();

        assert_eq!(outcome.removed_snapshots, 1);
        assert_eq!(outcome.cleaned_profiles, 1);
        assert_eq!(outcome.removed_cloud_objects, 1);
        assert!(
            DeletionRegistryRepository::new(operator.clone(), 2)
                .load()
                .await
                .unwrap()
                .deleted_games
                .contains_key("game")
        );
        assert!(
            SharedLibraryRepository::new(operator.clone(), 2)
                .load()
                .await
                .unwrap()
                .games
                .is_empty()
        );
        let profile = DeviceProfileRepository::new(operator.clone(), 2)
            .list()
            .await
            .unwrap()
            .remove(0);
        assert!(!profile.references_game("game", "Example"));
        assert!(
            CloudManifestRepository::new(operator.clone(), CLOUD_MANIFEST_PATH, 2)
                .load()
                .await
                .unwrap()
                .games
                .is_empty()
        );
        assert!(!local.path().join("game").exists());

        let retry = deletion.delete("game", "Example", true).await.unwrap();
        assert_eq!(retry.removed_snapshots, 0);
        assert_eq!(retry.removed_cloud_objects, 0);
    }

    #[tokio::test]
    async fn durable_marker_blocks_stale_resurrection_across_all_owner_files() {
        let (operator, local) = fixture().await;
        let stale_library = shared_library();
        let stale_profile: crate::config::DeviceProfile = serde_json::from_slice(
            &operator
                .read(&device_profile_path("pc"))
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        SharedGameDeletion::new(operator.clone(), local.path().into(), "pc".into(), 2)
            .delete("game", "Example", true)
            .await
            .unwrap();

        let empty_library = SharedLibraryRepository::new(operator.clone(), 2)
            .load()
            .await
            .unwrap();
        assert!(matches!(
            SharedLibraryRepository::new(operator.clone(), 2)
                .compare_replace(&empty_library, &stale_library)
                .await,
            Err(SharedLibraryRepositoryError::DeletedGame(game)) if game == "game"
        ));
        assert!(matches!(
            DeviceProfileRepository::new(operator.clone(), 2)
                .publish("pc", &stale_profile)
                .await,
            Err(DeviceProfileRepositoryError::DeletedGame(game)) if game == "game"
        ));
        assert!(matches!(
            CloudManifestRepository::new(operator, CLOUD_MANIFEST_PATH, 2)
                .mutate(|manifest| {
                    manifest.game_mut("game");
                    Ok(())
                })
                .await,
            Err(ManifestRepositoryError::DeletedGame(game)) if game == "game"
        ));
    }

    #[tokio::test]
    async fn confirmation_and_path_validation_fail_before_mutation() {
        let (operator, local) = fixture().await;
        let deletion =
            SharedGameDeletion::new(operator.clone(), local.path().into(), "pc".into(), 2);

        assert!(matches!(
            deletion.delete("game", "Example", false).await,
            Err(SharedGameDeletionError::ConfirmationRequired)
        ));
        assert!(matches!(
            deletion.delete("../other", "Unsafe", true).await,
            Err(SharedGameDeletionError::UnsafeGameId(_))
        ));
        assert!(
            DeletionRegistryRepository::new(operator, 2)
                .load()
                .await
                .unwrap()
                .deleted_games
                .is_empty()
        );
    }
}
