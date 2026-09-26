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
            auto_backup_limit: None,
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

#[test]
fn automatic_connection_retains_only_conflicts_and_explicit_choices_resolve_one_game() {
    let (_root, store, before) = fixture();
    let mut remote = remote_library(&before);
    let mut conflicting = before.shared_library.games[0].clone();
    conflicting.name = "Cloud version".into();
    remote.games.push(conflicting);
    store
        .connect_v2(
            &before.shared_library,
            &before.device_profiles["pc"],
            &before.local_state,
            &remote,
            "library-a",
            &before.local_state.cloud_settings,
        )
        .unwrap();
    let pending = store.load().unwrap();
    assert_eq!(pending.local_state.local_games, before.shared_library.games);
    assert_eq!(
        store
            .load_effective()
            .unwrap()
            .games
            .iter()
            .find(|game| game.storage_key == "local-game")
            .unwrap()
            .name,
        "Local adventure"
    );
    assert_eq!(
        pending.device_profiles["pc"].games["local-game"].save_units,
        before.device_profiles["pc"].games["local-game"].save_units
    );
    assert!(
        !pending.device_profiles["pc"]
            .without_local_games(&pending.local_state)
            .games
            .contains_key("local-game")
    );
    store
        .resolve_cloud_definitions(
            &pending.shared_library,
            &pending.device_profiles["pc"],
            &pending.local_state,
            &remote,
            &["local-game".into()],
        )
        .unwrap();
    assert!(store.load().unwrap().local_state.local_games.is_empty());
    assert_eq!(
        store
            .load_effective()
            .unwrap()
            .games
            .iter()
            .find(|game| game.storage_key == "local-game")
            .unwrap()
            .name,
        "Cloud version"
    );
}

#[test]
fn equal_definition_connects_without_a_choice_and_stale_connection_cannot_replace_local_edits() {
    let (_root, store, before) = fixture();
    store
        .connect_v2(
            &before.shared_library,
            &before.device_profiles["pc"],
            &before.local_state,
            &before.shared_library,
            "library-a",
            &before.local_state.cloud_settings,
        )
        .unwrap();
    assert!(store.load().unwrap().local_state.local_games.is_empty());
    assert!(matches!(
        store.connect_v2(
            &before.shared_library,
            &before.device_profiles["pc"],
            &before.local_state,
            &remote_library(&before),
            "library-b",
            &before.local_state.cloud_settings,
        ),
        Err(OwnerStoreError::JoinInputsChanged)
    ));
    assert_eq!(
        store
            .load()
            .unwrap()
            .local_state
            .cloud_library_id
            .as_deref(),
        Some("library-a")
    );
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
fn cutover_keeps_cloud_history_without_managing_remote_only_games() {
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
    let after = store.load().unwrap();
    assert_eq!(after.shared_library, remote);
    assert!(!after.device_profiles["pc"].games.contains_key("cloud-game"));
    assert!(after.device_profiles["pc"].games.contains_key("local-game"));
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
    let mut accepted = remote.clone();
    accepted.games.extend(before.shared_library.games.clone());
    store
        .reconcile_game_metadata(&store.load().unwrap().local_state, &accepted, &[], &[])
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
        .reconcile_game_metadata(&store.load().unwrap().local_state, &remote, &[], &[])
        .unwrap();
    assert_local_preserved(&store, &before);
    store
        .reconcile_game_metadata(&store.load().unwrap().local_state, &remote, &[], &[])
        .unwrap();
    assert_local_preserved(&store, &before);
}

#[test]
fn discovering_a_conflicting_local_id_keeps_the_local_version_until_a_choice() {
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
    let mut discovered = remote.clone();
    let mut conflicting = before.shared_library.games[0].clone();
    conflicting.name = "Different cloud definition".into();
    discovered.games.push(conflicting.clone());
    store
        .reconcile_game_metadata(&store.load().unwrap().local_state, &discovered, &[], &[])
        .unwrap();
    let accepted = store.load().unwrap();
    let effective = accepted.assemble_effective().unwrap();
    assert_eq!(
        effective
            .games
            .iter()
            .find(|game| game.storage_key == "local-game")
            .unwrap()
            .name,
        "Local adventure"
    );
    assert_eq!(effective.games.len(), 2);
    assert!(accepted.shared_library.games.contains(&conflicting));
    assert_eq!(accepted.local_state.local_games.len(), 1);
    assert_eq!(
        accepted.device_profiles["pc"].games["local-game"].save_units,
        profile.games["local-game"].save_units
    );
    store.merge_effective(&effective).unwrap();
    let round_trip = store.load().unwrap();
    assert_eq!(round_trip.shared_library, accepted.shared_library);
    assert_eq!(
        round_trip.local_state.local_games,
        accepted.local_state.local_games
    );
}

#[test]
fn pending_metadata_keeps_newer_edits_when_an_earlier_publication_finishes() {
    let (_root, store, before) = fixture();
    store
        .activate_v2(
            &before.shared_library,
            &before.device_profiles["pc"],
            "library-a",
        )
        .unwrap();
    let mut config = store.load_effective().unwrap();
    config.games[0].name = "First edit".into();
    store.merge_effective(&config).unwrap();
    let sent = store.load().unwrap().local_state;
    assert_eq!(
        sent.pending_game_metadata["local-game"]
            .base
            .as_ref()
            .unwrap()
            .name,
        "Local adventure"
    );
    assert_eq!(store.load().unwrap().shared_library, before.shared_library);
    config.games[0].name = "Second edit".into();
    config.games[0].auto_backup_limit = Some(9);
    store.merge_effective(&config).unwrap();
    let mut remote = before.shared_library.clone();
    remote.games[0].name = "First edit".into();
    remote.games[0].snapshot_retention = Some(super::super::SharedSnapshotRetentionPolicy {
        automatic_snapshots_per_branch: 4,
    });
    store
        .reconcile_game_metadata(&sent, &remote, &["local-game".into()], &[])
        .unwrap();
    let current = store.load().unwrap();
    let pending = &current.local_state.pending_game_metadata["local-game"];
    assert_eq!(pending.base.as_ref().unwrap().name, "First edit");
    assert_eq!(pending.desired.name, "Second edit");
    assert!(pending.desired.snapshot_retention.is_none());
    assert_eq!(
        current.assemble_effective().unwrap().games[0].auto_backup_limit,
        Some(9)
    );
    // A regular refresh cannot overwrite the still-pending local definition.
    store
        .reconcile_game_metadata(&current.local_state, &remote, &[], &[])
        .unwrap();
    assert_eq!(store.load_effective().unwrap().games[0].name, "Second edit");
    remote.games[0].name = "Second edit".into();
    store
        .reconcile_game_metadata(&current.local_state, &remote, &["local-game".into()], &[])
        .unwrap();
    assert!(
        store
            .load()
            .unwrap()
            .local_state
            .pending_game_metadata
            .is_empty()
    );
    assert_eq!(
        store.load().unwrap().shared_library.games[0]
            .snapshot_retention
            .unwrap()
            .automatic_snapshots_per_branch,
        4
    );
}

#[test]
fn reconnect_preserves_unsent_definitions_but_rejects_results_from_old_library() {
    let (_root, store, before) = fixture();
    store
        .activate_v2(
            &before.shared_library,
            &before.device_profiles["pc"],
            "library-a",
        )
        .unwrap();
    let mut config = store.load_effective().unwrap();
    config.games[0].name = "Unsent edit".into();
    store.merge_effective(&config).unwrap();
    let old = store.load().unwrap();
    let other = remote_library(&before);
    store
        .connect_v2(
            &old.shared_library,
            &old.device_profiles["pc"],
            &old.local_state,
            &other,
            "library-b",
            &old.local_state.cloud_settings,
        )
        .unwrap();
    let connected = store.load().unwrap();
    assert!(connected.local_state.pending_game_metadata.is_empty());
    assert!(
        connected
            .local_state
            .local_games
            .iter()
            .any(|game| game.name == "Unsent edit")
    );
    assert!(
        store
            .reconcile_game_metadata(
                &old.local_state,
                &old.shared_library,
                &["local-game".into()],
                &[]
            )
            .is_err()
    );
    assert_eq!(
        store
            .load()
            .unwrap()
            .local_state
            .cloud_library_id
            .as_deref(),
        Some("library-b")
    );
}

#[test]
fn global_deletion_clears_pending_metadata_without_republishing_it() {
    let (_root, store, before) = fixture();
    store
        .activate_v2(
            &before.shared_library,
            &before.device_profiles["pc"],
            "library-a",
        )
        .unwrap();
    let mut config = store.load_effective().unwrap();
    config.games[0].name = "Unsent edit".into();
    store.merge_effective(&config).unwrap();
    store
        .remove_shared_game("local-game", "Unsent edit")
        .unwrap();
    let after = store.load().unwrap();
    assert!(after.local_state.pending_game_metadata.is_empty());
    assert!(after.assemble_effective().unwrap().games.is_empty());
}
