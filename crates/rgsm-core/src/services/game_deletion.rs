use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::app_dirs::resolve_app_path;
use crate::cloud_sync::v2::{
    CLOUD_MANIFEST_PATH, CloudManifestRepository, DeletionRegistryRepository,
    DeviceProfileRepository, SharedGameDeletion, SharedGameDeletionOutcome,
    SharedLibraryRepository,
};
use crate::config::{CloudNamespaceGeneration, cloud_bootstrap_inputs, remove_shared_game};

use super::{CloudLibraryServiceError, ServiceContext, cloud_library_target::bound_v2_operator};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
pub struct DeletedCloudGameView {
    pub game_id: String,
    pub name: String,
    pub deletion_incomplete: bool,
}

impl ServiceContext {
    pub async fn deleted_cloud_games(
        &self,
    ) -> Result<Vec<DeletedCloudGameView>, CloudLibraryServiceError> {
        converge_local_deleted_games().await?;
        let (_, profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let operator = bound_v2_operator(&local_state).await?;
        let registry = DeletionRegistryRepository::new(operator.clone(), 3)
            .load()
            .await?;
        let shared = SharedLibraryRepository::new(operator.clone(), 3)
            .load()
            .await?;
        let manifest = CloudManifestRepository::new(operator.clone(), CLOUD_MANIFEST_PATH, 3)
            .load()
            .await?;
        let profiles = DeviceProfileRepository::new(operator.clone(), 3)
            .list()
            .await?;
        let local_root = profile.local_archive_root.as_deref().map(resolve_app_path);
        let mut games = Vec::with_capacity(registry.deleted_games.len());
        for (game_id, deletion) in &registry.deleted_games {
            let name = if deletion.name.is_empty() {
                game_id.clone()
            } else {
                deletion.name.clone()
            };
            let archive_prefix =
                crate::cloud_sync::v2::game_deletion::cloud_game_archive_prefix(game_id)?;
            let cloud_archives_remain = cloud_prefix_has_entries(&operator, &archive_prefix)
                .await
                .map_err(crate::preclude::BackendError::from)?;
            let deletion_incomplete = shared.games.iter().any(|game| game.storage_key == *game_id)
                || manifest.games.contains_key(game_id)
                || profiles.iter().any(|profile| {
                    !registry.deleted_profiles.contains_key(&profile.device.id)
                        && profile.references_game(game_id, &name)
                })
                || cloud_archives_remain
                || local_root
                    .as_ref()
                    .is_some_and(|root| root.join(game_id).exists());
            games.push(DeletedCloudGameView {
                game_id: game_id.clone(),
                name,
                deletion_incomplete,
            });
        }
        Ok(games)
    }

    pub async fn permanently_delete_cloud_game(
        &self,
        game_id: &str,
        confirmed: bool,
    ) -> Result<SharedGameDeletionOutcome, CloudLibraryServiceError> {
        let (library, profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let operator = bound_v2_operator(&local_state).await?;
        let registry = DeletionRegistryRepository::new(operator.clone(), 3)
            .load()
            .await?;
        let game_name = library
            .games
            .iter()
            .find(|game| game.storage_key == game_id)
            .map(|game| game.name.clone())
            .or_else(|| {
                registry
                    .deleted_games
                    .get(game_id)
                    .map(|deletion| deletion.name.clone())
            })
            .filter(|name| !name.is_empty())
            .ok_or_else(|| CloudLibraryServiceError::GameProfileNotFound(game_id.to_string()))?;
        let local_root = profile
            .local_archive_root
            .as_deref()
            .map(resolve_app_path)
            .ok_or(CloudLibraryServiceError::StorageLocationRequired)?;
        let outcome =
            SharedGameDeletion::new(operator, local_root, local_state.current_device_id, 3)
                .delete(game_id, &game_name, confirmed)
                .await?;
        remove_shared_game(game_id, &game_name)?;
        Ok(outcome)
    }
}

pub(crate) async fn converge_local_deleted_games() -> Result<(), CloudLibraryServiceError> {
    let (_, profile, local_state) = cloud_bootstrap_inputs()?;
    if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
        return Ok(());
    }
    let operator = bound_v2_operator(&local_state).await?;
    let registry = DeletionRegistryRepository::new(operator, 3).load().await?;
    for (game_id, deletion) in registry.deleted_games {
        if let Some(root) = profile.local_archive_root.as_deref().map(resolve_app_path) {
            crate::cloud_sync::v2::game_deletion::remove_local_game_directory(&root, &game_id)
                .await?;
        }
        let name = if deletion.name.is_empty() {
            game_id.as_str()
        } else {
            deletion.name.as_str()
        };
        remove_shared_game(&game_id, name)?;
    }
    Ok(())
}

async fn cloud_prefix_has_entries(
    operator: &opendal::Operator,
    prefix: &str,
) -> Result<bool, opendal::Error> {
    let mut lister = operator.lister(prefix).await?;
    Ok(lister.try_next().await?.is_some())
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use crate::backup::{ArchiveFormat, CreatedBy, Game, GameSnapshots, Snapshot, archive_path};
    use crate::cloud_sync::v2::{
        CLOUD_MANIFEST_PATH, CloudLibraryBootstrap, CloudManifestRepository,
        DeletionRegistryRepository, DeviceProfileRepository, SharedGameDeletionError,
        SharedLibraryRepository, SnapshotSyncCoordinator, cloud_archive_path,
    };
    use crate::cloud_sync::{Backend, CloudSettings, CloudSyncSessionConfig};
    use crate::config::{
        CloudNamespaceGeneration, Config, ConfigTestStateGuard, ConfigurationOwners, SharedLibrary,
        activate_cloud_namespace_v2, cloud_bootstrap_inputs, get_config, set_config_local,
    };
    use crate::device::get_current_device_id;
    use crate::hooks::HookPipeline;
    use tokio_util::sync::CancellationToken;

    use super::*;

    async fn assert_active_cloud_game(
        config: &Config,
        local_archive: &Path,
        cloud_archive: &str,
        second_device_id: &str,
    ) {
        assert!(
            get_config()
                .expect("effective configuration should reload")
                .games
                .iter()
                .any(|game| game.storage_key == "example-game")
        );
        let (_, active_profile, _) =
            cloud_bootstrap_inputs().expect("active local profile should reload");
        assert!(active_profile.games.contains_key("example-game"));
        assert_eq!(
            std::fs::read(local_archive).expect("local archive should remain readable"),
            b"service deletion archive bytes"
        );

        let operator = CloudSyncSessionConfig::from(&config.settings.cloud_settings)
            .get_op()
            .expect("fresh Fs operator should initialize");
        assert!(
            SharedLibraryRepository::new(operator.clone(), 2)
                .load()
                .await
                .expect("Shared Library should reload")
                .games
                .iter()
                .any(|game| game.storage_key == "example-game")
        );
        let profiles = DeviceProfileRepository::new(operator.clone(), 2)
            .list()
            .await
            .expect("Device Profiles should reload");
        assert!(
            profiles
                .iter()
                .find(|profile| profile.device.id == second_device_id)
                .expect("second Device Profile should remain")
                .games
                .contains_key("example-game")
        );
        assert!(
            CloudManifestRepository::new(operator.clone(), CLOUD_MANIFEST_PATH, 2)
                .load()
                .await
                .expect("Cloud Manifest should reload")
                .games
                .contains_key("example-game")
        );
        assert!(
            operator
                .exists(cloud_archive)
                .await
                .expect("cloud archive should remain")
        );
        assert!(
            !DeletionRegistryRepository::new(operator, 2)
                .load()
                .await
                .expect("Deletion Registry should reload")
                .deleted_games
                .contains_key("example-game")
        );
    }

    async fn assert_deleted_cloud_game(config: &Config, local_root: &Path, cloud_archive: &str) {
        assert!(
            get_config()
                .expect("effective configuration should reload")
                .games
                .iter()
                .all(|game| game.storage_key != "example-game")
        );
        let (_, active_profile, _) =
            cloud_bootstrap_inputs().expect("active local profile should reload");
        assert!(!active_profile.games.contains_key("example-game"));
        assert!(!local_root.join("example-game").exists());

        let operator = CloudSyncSessionConfig::from(&config.settings.cloud_settings)
            .get_op()
            .expect("fresh Fs operator should initialize");
        assert!(
            SharedLibraryRepository::new(operator.clone(), 2)
                .load()
                .await
                .expect("Shared Library should reload")
                .games
                .iter()
                .all(|game| game.storage_key != "example-game")
        );
        assert!(
            DeviceProfileRepository::new(operator.clone(), 2)
                .list()
                .await
                .expect("Device Profiles should reload")
                .iter()
                .all(|profile| !profile.games.contains_key("example-game"))
        );
        assert!(
            !CloudManifestRepository::new(operator.clone(), CLOUD_MANIFEST_PATH, 2)
                .load()
                .await
                .expect("Cloud Manifest should reload")
                .games
                .contains_key("example-game")
        );
        assert!(
            !operator
                .exists(cloud_archive)
                .await
                .expect("cloud archive absence should be observable")
        );
        assert!(
            DeletionRegistryRepository::new(operator, 2)
                .load()
                .await
                .expect("Deletion Registry should reload")
                .deleted_games
                .contains_key("example-game")
        );
    }

    #[test]
    fn permanently_delete_cloud_game() {
        let _config_lock = crate::config::lock_config_test_file();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime should initialize");
        runtime.block_on(async {
            const GAME_ID: &str = "example-game";
            const GAME_NAME: &str = "Example Game";
            const SNAPSHOT_ID: &str = "snapshot-a";
            const LIBRARY_ID: &str = "11111111-1111-4111-8111-111111111111";
            const ARCHIVE_BYTES: &[u8] = b"service deletion archive bytes";

            let _ = crate::app_dirs::get_app_data_dir();
            let cloud_root = temp_dir::TempDir::new().expect("temporary Fs root should initialize");
            let local_root =
                temp_dir::TempDir::new().expect("temporary local archive root should initialize");
            let current_device_id = get_current_device_id();
            let mut config = Config {
                backup_path: local_root.path().to_string_lossy().into_owned(),
                games: vec![Game {
                    name: GAME_NAME.to_string(),
                    storage_key: GAME_ID.to_string(),
                    save_paths: Vec::new(),
                    game_paths: Default::default(),
                    next_save_unit_id: 0,
                    cloud_sync_enabled: true,
                    auto_backup: None,
                    ludusavi_meta: None,
                    device_bindings: Default::default(),
                }],
                ..Config::default()
            };
            config.settings.cloud_settings = CloudSettings {
                backend: Backend::Fs,
                root_path: cloud_root.path().to_string_lossy().into_owned(),
                max_concurrency: 2,
                ..CloudSettings::default()
            };
            let _config_state =
                ConfigTestStateGuard::replace_with(&config).expect("config state should isolate");
            set_config_local(&config).expect("effective V2 configuration should persist");
            let (library, profile, _) =
                cloud_bootstrap_inputs().expect("persisted configuration should load");
            activate_cloud_namespace_v2(&library, &profile, LIBRARY_ID)
                .expect("configuration should activate the V2 namespace");
            assert_eq!(
                crate::config::cloud_namespace_generation()
                    .expect("namespace generation should load"),
                CloudNamespaceGeneration::V2
            );

            let operator = CloudSyncSessionConfig::from(&config.settings.cloud_settings)
                .get_op()
                .expect("Fs operator should initialize");
            let empty_library = SharedLibrary {
                schema_version: library.schema_version,
                games: Vec::new(),
            };
            CloudLibraryBootstrap::new(operator.clone(), 2)
                .create_empty(
                    &crate::cloud_sync::v2::CloudNamespaceDescriptor::with_library_id(LIBRARY_ID),
                    &empty_library,
                    &profile,
                )
                .await
                .expect("empty Cloud Namespace should bootstrap");
            SharedLibraryRepository::new(operator.clone(), 2)
                .compare_replace(&empty_library, &library)
                .await
                .expect("Shared Game should publish");
            DeviceProfileRepository::new(operator.clone(), 2)
                .publish(current_device_id, &profile)
                .await
                .expect("acting Device Profile should publish");
            let second_device_id = "device-b".to_string();
            let mut second_profile = ConfigurationOwners::from_legacy(&config, &second_device_id)
                .device_profiles
                .remove(&second_device_id)
                .expect("second Device Profile should initialize");
            second_profile.local_archive_root = Some(
                local_root
                    .path()
                    .join("device-b")
                    .to_string_lossy()
                    .into_owned(),
            );
            DeviceProfileRepository::new(operator.clone(), 2)
                .publish(&second_device_id, &second_profile)
                .await
                .expect("second Device Profile should publish");

            let snapshot = Snapshot {
                date: SNAPSHOT_ID.to_string(),
                describe: "service deletion snapshot".to_string(),
                path: format!("{SNAPSHOT_ID}.zip"),
                archive_format: ArchiveFormat::Zip,
                size: ARCHIVE_BYTES.len() as u64,
                parent: None,
                archive_hash: None,
                device_id: Some(current_device_id.clone()),
                created_by: CreatedBy::Manual,
            };
            let local_archive = archive_path(
                &local_root.path().join(GAME_ID),
                SNAPSHOT_ID,
                ArchiveFormat::Zip,
            );
            std::fs::create_dir_all(
                local_archive
                    .parent()
                    .expect("archive path should have parent"),
            )
            .expect("archive directory should initialize");
            std::fs::write(&local_archive, ARCHIVE_BYTES).expect("archive bytes should persist");
            let mut snapshots = GameSnapshots::new(GAME_NAME);
            snapshots.backups = vec![snapshot.clone()];
            snapshots.set_head_for_device(current_device_id.clone(), Some(SNAPSHOT_ID.to_string()));
            SnapshotSyncCoordinator::new(
                operator.clone(),
                local_root.path().to_path_buf(),
                current_device_id.clone(),
                local_root.path().join("materialization-progress.json"),
                2,
            )
            .reconcile_game(
                GAME_ID,
                &snapshots,
                0,
                &Default::default(),
                &CancellationToken::new(),
            )
            .await
            .expect("verified archive should populate the Cloud Manifest");
            let cloud_archive = cloud_archive_path(GAME_ID, SNAPSHOT_ID, ArchiveFormat::Zip)
                .expect("cloud archive path should be valid");
            assert!(
                operator
                    .exists(&cloud_archive)
                    .await
                    .expect("cloud archive should exist")
            );

            let service = ServiceContext::new(Arc::new(HookPipeline::new(vec![])));
            let unconfirmed = service
                .permanently_delete_cloud_game(GAME_ID, false)
                .await
                .expect_err("permanent deletion should require confirmation");
            assert!(matches!(
                unconfirmed,
                CloudLibraryServiceError::SharedGameDeletion(
                    SharedGameDeletionError::ConfirmationRequired
                )
            ));
            assert_active_cloud_game(&config, &local_archive, &cloud_archive, &second_device_id)
                .await;

            service
                .permanently_delete_cloud_game(GAME_ID, true)
                .await
                .expect("confirmed deletion should converge every owner");

            assert_deleted_cloud_game(&config, local_root.path(), &cloud_archive).await;

            service
                .permanently_delete_cloud_game(GAME_ID, true)
                .await
                .expect("repeated confirmed deletion should converge idempotently");
            assert_deleted_cloud_game(&config, local_root.path(), &cloud_archive).await;
        });
    }
}
