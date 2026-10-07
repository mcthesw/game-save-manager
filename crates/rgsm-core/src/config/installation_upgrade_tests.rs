use super::*;
use crate::config::owner_store::OwnerStore;
use crate::device::get_current_device_id;

fn fixture() -> Config {
    let mut config = Config::default();
    for (id, path) in [
        (get_current_device_id().clone(), "D:/Game"),
        ("another-device".into(), "E:/Game"),
    ] {
        let device: Device = serde_json::from_value(serde_json::json!({
            "id":id, "name":id, "resources":[{"id":7,"source":"manual","kind":{
                "type":"gameInstallation","root_id":0,"store":"steam","install_dir":"Game",
                "path":path,"store_game_id":"42"
            }}]
        }))
        .unwrap();
        config.devices.insert(id, device);
    }
    let mut game: Game = serde_json::from_value(serde_json::json!({
        "name":"Game", "storage_key":"game", "save_paths":[],
    }))
    .unwrap();
    for id in config.devices.keys() {
        game.device_bindings.insert(
            id.clone(),
            serde_json::from_value(serde_json::json!({"installationIds":[7]})).unwrap(),
        );
    }
    config.games.push(game);
    config
}

#[test]
fn explicit_installations_migrate_by_device_and_never_share_resource_ids() {
    let mut config = fixture();
    assert!(migrate(&mut config));
    assert_eq!(
        config.games[0].device_bindings[get_current_device_id()]
            .installation_path
            .as_deref(),
        Some("D:/Game")
    );
    assert_eq!(
        config.games[0].device_bindings["another-device"]
            .installation_path
            .as_deref(),
        Some("E:/Game")
    );
    assert!(
        config.games[0]
            .device_bindings
            .values()
            .all(|binding| binding.installation_ids.is_none())
    );
    assert!(
        config
            .devices
            .values()
            .all(|device| device.game_installations().next().is_none())
    );
    assert!(!migrate(&mut config));
}

#[test]
fn ambiguous_and_unassigned_installations_stay_pending_until_chosen() {
    let mut config = fixture();
    let device_id = get_current_device_id().clone();
    let device = config.devices.get_mut(&device_id).unwrap();
    let mut second = device.resources[0].clone();
    second.id = 8;
    if let DeviceResourceKind::GameInstallation { path, .. } = &mut second.kind {
        *path = "F:/Game".into();
    }
    device.resources.push(second.clone());
    let mut orphan = second;
    orphan.id = 9;
    if let DeviceResourceKind::GameInstallation { path, .. } = &mut orphan.kind {
        *path = "G:/Unassigned".into();
    }
    device.resources.push(orphan);
    config.games[0]
        .device_bindings
        .get_mut(&device_id)
        .unwrap()
        .installation_ids = Some(vec!["resource:7".into(), "resource:8".into()]);
    migrate(&mut config);
    assert!(
        config.games[0].device_bindings[&device_id]
            .installation_path
            .is_none()
    );
    assert_eq!(config.devices[&device_id].resources.len(), 3);
    let options = crate::services::game_location_options(
        &config.games[0],
        Some(&config.devices[&device_id]),
        false,
    );
    assert_eq!(options.installations, vec!["D:/Game", "F:/Game"]);
    // The player chooses one path; the unresolved records cease to be a second
    // source of installation settings. Unassigned data remains available.
    config.games[0]
        .device_bindings
        .get_mut(&device_id)
        .unwrap()
        .installation_path = Some("F:/Game".into());
    migrate(&mut config);
    assert_eq!(config.devices[&device_id].resources.len(), 1);
    assert_eq!(config.devices[&device_id].resources[0].id, 9);
}

#[test]
fn known_installation_path_wins_over_an_old_explicit_reference() {
    let mut config = fixture();
    let binding = config.games[0]
        .device_bindings
        .get_mut(get_current_device_id())
        .unwrap();
    binding.installation_path = Some("X:/Moved game".into());
    migrate(&mut config);
    assert_eq!(
        config.games[0].device_bindings[get_current_device_id()]
            .installation_path
            .as_deref(),
        Some("X:/Moved game")
    );
    assert!(config.devices[get_current_device_id()].resources.is_empty());
}

#[test]
fn owned_local_profiles_upgrade_once_and_keep_a_recoverable_original() {
    let directory = temp_dir::TempDir::new().unwrap();
    let store = OwnerStore::new(directory.path().into());
    store.initialize_from_legacy(&fixture()).unwrap();
    assert!(store.upgrade_local_installations().unwrap());
    let config = store.load_effective().unwrap();
    assert_eq!(
        config.games[0].device_bindings[get_current_device_id()]
            .installation_path
            .as_deref(),
        Some("D:/Game")
    );
    let original: super::super::ConfigurationOwners = serde_json::from_slice(
        &std::fs::read(
            directory
                .path()
                .join("GameSaveManager.installations-before-upgrade.json.bak"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        original.device_profiles[get_current_device_id()]
            .device
            .resources
            .len(),
        1
    );
    assert!(!store.upgrade_local_installations().unwrap());
}
