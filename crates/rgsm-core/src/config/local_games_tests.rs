use super::*;
use crate::backup::{Game, SaveUnit, SaveUnitType};
use crate::device::Device;

fn fixture() -> (temp_dir::TempDir, OwnerStore, ConfigurationOwners) {
    let root = temp_dir::TempDir::new().unwrap();
    let store = OwnerStore::new(root.path().to_path_buf());
    let config = Config {
        games: vec![Game {
            name: "Local adventure".into(),
            storage_key: "local-game".into(),
            save_paths: vec![SaveUnit::concrete(
                7,
                SaveUnitType::Folder,
                HashMap::from([("pc".into(), "D:/Saves[Main]".into())]),
                true,
                false,
            )],
            next_save_unit_id: 8,
            game_paths: HashMap::from([("pc".into(), "D:/Games/game.exe".into())]),
            cloud_sync_enabled: false,
            auto_backup: None,
            ludusavi_meta: None,
            device_bindings: HashMap::new(),
        }],
        devices: HashMap::from([(
            "pc".into(),
            Device {
                id: "pc".into(),
                name: "My PC".into(),
                resources: vec![],
                next_resource_id: 0,
            },
        )]),
        ..Default::default()
    };
    let owners = ConfigurationOwners::from_legacy(&config, &"pc".into());
    store.write(&owners).unwrap();
    (root, store, owners)
}

fn remote_library(before: &ConfigurationOwners) -> SharedLibrary {
    let mut library = before.shared_library.clone();
    library.games[0].name = "Cloud adventure".into();
    library.games[0].storage_key = "cloud-game".into();
    library
}

fn assert_local_preserved(store: &OwnerStore, before: &ConfigurationOwners) {
    let effective = store.load_effective().unwrap();
    let original = before.assemble_effective().unwrap();
    let local = effective
        .games
        .iter()
        .find(|game| game.storage_key == "local-game")
        .expect("accepting cloud definitions must retain the local game");
    assert_eq!(
        serde_json::to_value(local).unwrap(),
        serde_json::to_value(&original.games[0]).unwrap()
    );
    assert!(
        effective
            .games
            .iter()
            .any(|game| game.storage_key == "cloud-game")
    );
    assert_eq!(store.load().unwrap().shared_library.games.len(), 1);
}

#[test]
fn joining_cloud_retains_local_only_games_and_their_settings() {
    let (_root, store, before) = fixture();
    let remote = remote_library(&before);
    let profile = &before.device_profiles["pc"];
    store
        .activate_join_v2(
            &before.shared_library,
            profile,
            &remote,
            &profile.for_shared_library(&remote),
            "library-a",
        )
        .unwrap();
    assert_local_preserved(&store, &before);
    let effective = store.load_effective().unwrap();
    store.merge_effective(&effective).unwrap();
    assert_local_preserved(&store, &before);
    store.replace_effective(&effective).unwrap();
    assert_local_preserved(&store, &before);
    let mut edited = effective;
    edited
        .games
        .iter_mut()
        .find(|game| game.storage_key == "local-game")
        .unwrap()
        .name = "Renamed local".into();
    store.merge_effective(&edited).unwrap();
    assert_eq!(
        store.load().unwrap().local_state.local_games[0].name,
        "Renamed local"
    );
}

#[test]
fn cutover_retains_local_games_omitted_from_the_remote_library() {
    let (_root, store, before) = fixture();
    let remote = remote_library(&before);
    let profile = &before.device_profiles["pc"];
    let profiles = HashMap::from([("pc".into(), profile.for_shared_library(&remote))]);
    store
        .activate_cutover_v2(
            &before.shared_library,
            profile,
            &remote,
            &profiles,
            "library-a",
        )
        .unwrap();
    assert_local_preserved(&store, &before);
}

#[test]
fn local_profile_projection_and_explicit_deletion_keep_the_boundary() {
    let (_root, store, before) = fixture();
    let remote = remote_library(&before);
    let profile = &before.device_profiles["pc"];
    store
        .activate_join_v2(
            &before.shared_library,
            profile,
            &remote,
            &profile.for_shared_library(&remote),
            "library-a",
        )
        .unwrap();
    let current = store.load().unwrap();
    let published = current.device_profiles["pc"].without_local_games(&current.local_state);
    assert!(!published.games.contains_key("local-game"));
    assert!(published.games.contains_key("cloud-game"));
    assert!(
        current.device_profiles["pc"]
            .games
            .contains_key("local-game")
    );

    store
        .remove_shared_game("local-game", "Local adventure")
        .unwrap();
    let after = store.load().unwrap();
    assert!(after.local_state.local_games.is_empty());
    assert!(!after.device_profiles["pc"].games.contains_key("local-game"));
    assert!(
        !after
            .assemble_effective()
            .unwrap()
            .games
            .iter()
            .any(|game| game.storage_key == "local-game")
    );
}

#[test]
fn accepted_local_identity_is_not_duplicated_and_preserves_its_paths() {
    let (_root, store, before) = fixture();
    let remote = remote_library(&before);
    let profile = &before.device_profiles["pc"];
    store
        .activate_join_v2(
            &before.shared_library,
            profile,
            &remote,
            &profile.for_shared_library(&remote),
            "library-a",
        )
        .unwrap();
    let current = store.load().unwrap();
    let mut accepted = remote.clone();
    accepted.games.extend(before.shared_library.games.clone());
    let current_profile = &current.device_profiles["pc"];
    store
        .accept_remote_shared_library(
            &remote,
            current_profile,
            &accepted,
            &current_profile.for_shared_library(&accepted),
            "library-a",
        )
        .unwrap();
    let after = store.load().unwrap();
    assert!(after.local_state.local_games.is_empty());
    assert_eq!(after.assemble_effective().unwrap().games.len(), 2);
    assert_eq!(
        after.device_profiles["pc"].games["local-game"].save_units,
        profile.games["local-game"].save_units
    );
}

#[test]
fn reconnect_and_refresh_retain_local_games_without_publishing_them() {
    let (_root, store, before) = fixture();
    let profile = &before.device_profiles["pc"];
    store
        .activate_v2(&before.shared_library, profile, "library-a")
        .unwrap();
    let remote = remote_library(&before);
    store
        .accept_remote_shared_library(
            &before.shared_library,
            profile,
            &remote,
            &profile.for_shared_library(&remote),
            "library-b",
        )
        .unwrap();
    assert_local_preserved(&store, &before);
    let current = store.load().unwrap();
    let current_profile = &current.device_profiles["pc"];
    store
        .accept_remote_shared_library(
            &remote,
            current_profile,
            &remote,
            &current_profile.for_shared_library(&remote),
            "library-b",
        )
        .unwrap();
    assert_local_preserved(&store, &before);
}
