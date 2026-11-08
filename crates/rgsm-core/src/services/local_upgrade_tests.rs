use std::{io::Write, sync::Arc};

use super::*;
use crate::{
    backup::{ArchiveFormat, GameSnapshots, Snapshot},
    cloud_sync::{
        CloudSettings, CloudSyncSessionConfig,
        v2::{CloudLibraryBootstrap, CloudNamespaceDescriptor},
    },
    config::{ConfigTestStateGuard, ConfigurationOwners, set_config_local},
    hooks::HookPipeline,
    services::ServiceContext,
};

fn write_zip(path: &std::path::Path) -> Vec<u8> {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    zip.start_file("profile.sav", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"progress").unwrap();
    zip.finish().unwrap();
    std::fs::read(path).unwrap()
}

fn save_unit() -> serde_json::Value {
    serde_json::json!({"id": 7, "source": {"type": "devicePaths", "unit_type": "File",
        "paths": {crate::device::get_current_device_id().to_string(): "X:/Uninstalled/profile.sav"}}})
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn legacy_games_with_cloud_sync_keep_their_archives() {
    let mut config = Config {
        games: serde_json::from_value(serde_json::json!([
            {"name": "Synced", "storage_key": "synced", "save_paths": [], "cloud_sync_enabled": true},
            {"name": "Offline", "storage_key": "offline", "save_paths": [], "cloud_sync_enabled": false}
        ]))
        .unwrap(),
        ..Config::default()
    };
    let owners = ConfigurationOwners::from_legacy(&config, &"device".into());
    let names = |config: &Config| {
        games_outside_cloud_library(config, &owners.shared_library, &owners.local_state)
            .into_iter()
            .map(|game| game.storage_key)
            .collect::<Vec<_>>()
    };

    assert_eq!(names(&config), ["synced", "offline"]);
    config.settings.cloud_settings.backend = Backend::Fs;
    assert_eq!(names(&config), ["offline"]);
}

#[test]
fn a_disabled_backend_keeps_the_library_binding_and_its_archives() {
    let config = Config {
        games: serde_json::from_value(serde_json::json!([
            {"name": "Shared", "storage_key": "shared", "save_paths": []},
            {"name": "Local", "storage_key": "local", "save_paths": []}
        ]))
        .unwrap(),
        ..Config::default()
    };
    let mut owners = ConfigurationOwners::from_legacy(&config, &"device".into());
    let names = |owners: &ConfigurationOwners| {
        games_outside_cloud_library(&config, &owners.shared_library, &owners.local_state)
            .into_iter()
            .map(|game| game.storage_key)
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&owners), ["shared", "local"]);

    owners.local_state.cloud_namespace_generation = CloudNamespaceGeneration::V2;
    owners.local_state.cloud_library_id = Some("library".into());
    owners.local_state.local_games = owners.shared_library.games[1..].to_vec();

    assert!(matches!(
        owners.local_state.cloud_settings.backend,
        Backend::Disabled
    ));
    assert_eq!(names(&owners), ["local"]);
}

#[test]
fn connected_library_games_stay_zip_and_keep_syncing_after_local_upgrade() {
    let _lock = crate::config::lock_config_test_file();
    let cloud = temp_dir::TempDir::new().unwrap();
    let archives = temp_dir::TempDir::new().unwrap();
    let mut config = Config {
        backup_path: archives.path().to_string_lossy().into_owned(),
        games: serde_json::from_value(serde_json::json!([
            {"name": "Local", "storage_key": "local", "save_paths": [save_unit()]},
            {"name": "Shared", "storage_key": "shared", "save_paths": [save_unit()]}
        ]))
        .unwrap(),
        ..Config::default()
    };
    config.settings.cloud_settings = CloudSettings {
        backend: Backend::Fs,
        root_path: cloud.path().to_string_lossy().into_owned(),
        ..CloudSettings::default()
    };
    let _guard = ConfigTestStateGuard::replace_with(&config).unwrap();
    set_config_local(&config).unwrap();
    runtime().block_on(async {
        let (mut library, other_profile, _) = cloud_bootstrap_inputs().unwrap();
        library.games.retain(|game| game.storage_key == "shared");
        let operator = CloudSyncSessionConfig::from(&config.settings.cloud_settings)
            .get_op()
            .unwrap();
        CloudLibraryBootstrap::new(operator, 2)
            .create_empty(
                &CloudNamespaceDescriptor::default(),
                &library,
                &other_profile,
            )
            .await
            .unwrap();
        let service = ServiceContext::new(Arc::new(HookPipeline::new(vec![])));
        service.connect_cloud_library().await.unwrap();

        let (_, _, state) = cloud_bootstrap_inputs().unwrap();
        assert!(state.is_local_game("local"));
        let mut originals = Vec::new();
        for game in &get_config().unwrap().games {
            let path = archives.path().join(&game.storage_key).join("first.zip");
            originals.push((game.storage_key.clone(), write_zip(&path)));
            let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
                "date": "first", "describe": "", "path": path
            }))
            .unwrap();
            let mut catalog = GameSnapshots::new(&game.name);
            catalog.backups.push(snapshot);
            game.set_game_snapshots_info(&catalog).unwrap();
        }

        local_archive_upgrade(LocalUpgradeAction::Start).unwrap();
        while local_archive_upgrade(LocalUpgradeAction::Step)
            .unwrap()
            .remaining
            > 0
        {}

        let snapshot_of = |id: &str| {
            get_config()
                .unwrap()
                .games
                .iter()
                .find(|game| game.storage_key == id)
                .unwrap()
                .get_game_snapshots_info()
                .unwrap()
                .backups
                .remove(0)
        };
        let shared = snapshot_of("shared");
        assert_eq!(shared.archive_format, ArchiveFormat::Zip);
        assert_eq!(
            std::fs::read(archives.path().join("shared/first.zip")).unwrap(),
            originals[1].1
        );
        assert_eq!(snapshot_of("local").archive_format, ArchiveFormat::SevenZ);

        // The Library can still enable, review and materialize the untouched game.
        service
            .set_game_cloud_policy(
                "shared",
                true,
                crate::config::SyncMode::Manual,
                crate::config::InitialCatchUpPolicy::DownloadExisting,
                None,
                &tokio_util::sync::CancellationToken::new(),
            )
            .await
            .unwrap();
        service.preview_materialize_all().await.unwrap();
        assert_eq!(snapshot_of("shared").archive_format, ArchiveFormat::Zip);
    });
}

#[test]
fn recompressed_local_archives_of_shared_games_are_restored_by_download() {
    let _lock = crate::config::lock_config_test_file();
    let cloud = temp_dir::TempDir::new().unwrap();
    let archives = temp_dir::TempDir::new().unwrap();
    let mut config = Config {
        backup_path: archives.path().to_string_lossy().into_owned(),
        games: serde_json::from_value(serde_json::json!([
            {"name": "Shared", "storage_key": "shared", "save_paths": [save_unit()]}
        ]))
        .unwrap(),
        ..Config::default()
    };
    config.settings.cloud_settings = CloudSettings {
        backend: Backend::Fs,
        root_path: cloud.path().to_string_lossy().into_owned(),
        ..CloudSettings::default()
    };
    let _guard = ConfigTestStateGuard::replace_with(&config).unwrap();
    set_config_local(&config).unwrap();
    runtime().block_on(async {
        let (library, profile, _) = cloud_bootstrap_inputs().unwrap();
        let operator = CloudSyncSessionConfig::from(&config.settings.cloud_settings)
            .get_op()
            .unwrap();
        CloudLibraryBootstrap::new(operator, 2)
            .create_empty(&CloudNamespaceDescriptor::default(), &library, &profile)
            .await
            .unwrap();
        let service = ServiceContext::new(Arc::new(HookPipeline::new(vec![])));
        service.connect_cloud_library().await.unwrap();

        let game = get_config().unwrap().games.remove(0);
        let dir = archives.path().join("shared");
        let ids = ["first", "second"];
        let mut catalog = GameSnapshots::new(&game.name);
        let mut originals = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            let path = dir.join(format!("{id}.zip"));
            originals.push(write_zip(&path));
            catalog.backups.push(
                serde_json::from_value(serde_json::json!({
                    "date": id, "describe": "", "path": path,
                    "parent": index.checked_sub(1).map(|previous| ids[previous]),
                }))
                .unwrap(),
            );
        }
        game.set_game_snapshots_info(&catalog).unwrap();
        for id in ids {
            service.upload_cloud_archive("shared", id).await.unwrap();
        }

        // A local format upgrade that predates the Cloud Library protection:
        // same snapshots, repacked as hashed 7z archives the Cloud never saw.
        let mut upgraded = catalog.clone();
        for snapshot in &mut upgraded.backups {
            let name = format!("{}_aabbccddeeff.7z", snapshot.date);
            std::fs::remove_file(&snapshot.path).unwrap();
            std::fs::write(dir.join(&name), b"repacked").unwrap();
            snapshot.archive_format = ArchiveFormat::SevenZ;
            snapshot.path = dir.join(&name).to_string_lossy().into_owned();
            snapshot.archive_name = Some(name);
            snapshot.size = 8;
            snapshot.archive_hash = Some("1111111111111111".into());
        }
        game.set_game_snapshots_info(&upgraded).unwrap();

        service
            .download_cloud_archive("shared", "first")
            .await
            .unwrap();
        let rows = game.get_game_snapshots_info().unwrap().backups;
        assert_eq!(rows[0].archive_format, ArchiveFormat::Zip);
        assert_eq!(rows[0].archive_name, None);
        assert_eq!(std::fs::read(dir.join("first.zip")).unwrap(), originals[0]);
        assert!(!dir.join("first_aabbccddeeff.7z").exists());
        assert_eq!(rows[1].archive_format, ArchiveFormat::SevenZ);
        assert!(dir.join("second_aabbccddeeff.7z").exists());

        service
            .materialize_all_cloud_archives(&tokio_util::sync::CancellationToken::new())
            .await
            .unwrap();
        let rows = game.get_game_snapshots_info().unwrap().backups;
        assert!(
            rows.iter()
                .all(|row| row.archive_format == ArchiveFormat::Zip)
        );
        assert_eq!(std::fs::read(dir.join("second.zip")).unwrap(), originals[1]);
        assert!(!dir.join("second_aabbccddeeff.7z").exists());
    });
}
