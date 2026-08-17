use std::collections::HashMap;

use super::{
    CloudNamespaceGeneration, Config, ConfigurationOwners, DeviceGameProfile, OwnershipError,
    QuickActionsSettings, SharedSnapshotRetentionPolicy, SyncMode,
};
use crate::backup::{Game, SaveUnit, SaveUnitType};
use crate::device::{Device, DeviceId};

fn device(id: &str, name: &str) -> Device {
    Device {
        id: id.to_string(),
        name: name.to_string(),
        resources: Vec::new(),
        next_resource_id: 0,
    }
}

fn dual_device_config() -> (Config, DeviceId, DeviceId) {
    let windows_id = "windows-pc".to_string();
    let deck_id = "steam-deck".to_string();
    let mut save_paths = HashMap::new();
    save_paths.insert(
        windows_id.clone(),
        r"C:\Users\Player\Saved Games\Example".to_string(),
    );
    save_paths.insert(
        deck_id.clone(),
        "/home/deck/.local/share/example".to_string(),
    );
    let mut game_paths = HashMap::new();
    game_paths.insert(windows_id.clone(), r"C:\Games\Example\game.exe".to_string());
    game_paths.insert(deck_id.clone(), "/home/deck/Games/Example/game".to_string());
    let game = Game {
        name: "Example Game".to_string(),
        storage_key: "example-game".to_string(),
        save_paths: vec![SaveUnit::concrete(
            7,
            SaveUnitType::Folder,
            save_paths,
            true,
            false,
        )],
        game_paths,
        next_save_unit_id: 8,
        cloud_sync_enabled: true,
        auto_backup: None,
        ludusavi_meta: None,
        device_bindings: HashMap::new(),
    };
    let config = Config {
        backup_path: r"D:\Backups".to_string(),
        games: vec![game],
        quick_action: QuickActionsSettings {
            quick_action_game_id: Some("example-game".to_string()),
            ..Default::default()
        },
        devices: HashMap::from([
            (windows_id.clone(), device(&windows_id, "Windows PC")),
            (deck_id.clone(), device(&deck_id, "Steam Deck")),
        ]),
        ..Default::default()
    };
    (config, windows_id, deck_id)
}

#[test]
fn legacy_projection_separates_shared_data_from_device_paths() {
    let (config, windows_id, deck_id) = dual_device_config();

    let owners = ConfigurationOwners::from_legacy(&config, &windows_id);
    let shared = serde_json::to_string(&owners.shared_library).unwrap();

    assert!(!shared.contains(r"C:\Users"));
    assert!(!shared.contains("/home/deck"));
    let windows_game = &owners.device_profiles[&windows_id].games["example-game"];
    let deck_game = &owners.device_profiles[&deck_id].games["example-game"];
    assert_eq!(
        windows_game.save_units[&7].path.as_deref(),
        Some(r"C:\Users\Player\Saved Games\Example")
    );
    assert_eq!(
        deck_game.save_units[&7].path.as_deref(),
        Some("/home/deck/.local/share/example")
    );
    assert_eq!(
        owners.device_profiles[&windows_id]
            .local_archive_root
            .as_deref(),
        Some(r"D:\Backups")
    );
    assert!(
        owners.device_profiles[&deck_id]
            .local_archive_root
            .is_none()
    );
}

#[test]
fn owners_round_trip_back_to_equivalent_effective_config() {
    let (config, windows_id, _) = dual_device_config();
    let owners = ConfigurationOwners::from_legacy(&config, &windows_id);
    let persisted = serde_json::to_value(&owners).unwrap();
    let reloaded: ConfigurationOwners = serde_json::from_value(persisted).unwrap();

    let effective = reloaded.assemble_effective().unwrap();

    assert_eq!(
        serde_json::to_value(effective).unwrap(),
        serde_json::to_value(config).unwrap()
    );
}

#[test]
fn legacy_owner_json_defaults_to_v1_generation() {
    let (config, windows_id, _) = dual_device_config();
    let owners = ConfigurationOwners::from_legacy(&config, &windows_id);
    let mut persisted = serde_json::to_value(&owners).unwrap();
    persisted["local_state"]
        .as_object_mut()
        .unwrap()
        .remove("cloud_namespace_generation");

    let reloaded: ConfigurationOwners = serde_json::from_value(persisted).unwrap();

    assert_eq!(
        reloaded.local_state.cloud_namespace_generation,
        CloudNamespaceGeneration::LegacyV1
    );
}

#[test]
fn legacy_sync_mode_aliases_become_presets_and_infer_enablement() {
    let snapshot: DeviceGameProfile = serde_json::from_value(serde_json::json!({
        "visible": true,
        "sync_mode": "snapshot_sync",
        "game_path": null,
        "binding": null,
        "auto_backup": null,
        "save_units": {}
    }))
    .unwrap();
    assert_eq!(snapshot.sync_mode, SyncMode::CloudBackup);
    assert!(snapshot.cloud_sync_enabled);

    let live: DeviceGameProfile = serde_json::from_value(serde_json::json!({
        "visible": true,
        "sync_mode": "live_save_sync",
        "game_path": null,
        "binding": null,
        "auto_backup": null,
        "save_units": {}
    }))
    .unwrap();
    assert_eq!(live.sync_mode, SyncMode::MultiDeviceSync);
    assert!(live.cloud_sync_enabled);

    let manual: DeviceGameProfile = serde_json::from_value(serde_json::json!({
        "visible": true,
        "sync_mode": "manual",
        "game_path": null,
        "binding": null,
        "auto_backup": null,
        "save_units": {}
    }))
    .unwrap();
    assert_eq!(manual.sync_mode, SyncMode::Manual);
    assert!(!manual.cloud_sync_enabled);

    let remembered: DeviceGameProfile = serde_json::from_value(serde_json::json!({
        "visible": true,
        "cloud_sync_enabled": false,
        "sync_mode": "cloud_backup",
        "game_path": null,
        "binding": null,
        "auto_backup": null,
        "save_units": {}
    }))
    .unwrap();
    assert_eq!(remembered.sync_mode, SyncMode::CloudBackup);
    assert!(!remembered.cloud_sync_enabled);
}

#[test]
fn assembly_requires_the_selected_device_profile() {
    let (config, windows_id, _) = dual_device_config();
    let mut owners = ConfigurationOwners::from_legacy(&config, &windows_id);
    owners.device_profiles.remove(&windows_id);

    let result = owners.assemble_effective();

    assert!(matches!(
        result,
        Err(OwnershipError::MissingDeviceProfile(id)) if id == windows_id
    ));
}

#[test]
fn normal_save_preserves_other_device_private_profile_fields() {
    let (config, windows_id, deck_id) = dual_device_config();
    let mut owners = ConfigurationOwners::from_legacy(&config, &windows_id);
    owners
        .device_profiles
        .get_mut(&deck_id)
        .unwrap()
        .quick_action
        .quick_action_game_id = Some("deck-only-selection".to_string());
    owners
        .device_profiles
        .get_mut(&deck_id)
        .unwrap()
        .behavior
        .max_extra_backup_count = 37;
    let mut edited = owners.assemble_effective().unwrap();
    edited.quick_action.quick_action_game_id = Some("windows-selection".to_string());
    edited.settings.max_extra_backup_count = 9;

    owners.merge_effective(&edited).unwrap();

    let windows = &owners.device_profiles[&windows_id];
    let deck = &owners.device_profiles[&deck_id];
    assert_eq!(
        windows.quick_action.quick_action_game_id.as_deref(),
        Some("windows-selection")
    );
    assert_eq!(windows.behavior.max_extra_backup_count, 9);
    assert_eq!(
        deck.quick_action.quick_action_game_id.as_deref(),
        Some("deck-only-selection")
    );
    assert_eq!(deck.behavior.max_extra_backup_count, 37);
}

#[test]
fn normal_save_preserves_shared_snapshot_retention() {
    let (config, windows_id, _) = dual_device_config();
    let mut owners = ConfigurationOwners::from_legacy(&config, &windows_id);
    owners.shared_library.games[0].snapshot_retention = Some(SharedSnapshotRetentionPolicy {
        automatic_snapshots_per_branch: 7,
    });
    let mut edited = owners.assemble_effective().unwrap();
    edited.games[0].name = "Edited display name".into();

    owners.merge_effective(&edited).unwrap();

    assert_eq!(
        owners.shared_library.games[0].snapshot_retention,
        Some(SharedSnapshotRetentionPolicy {
            automatic_snapshots_per_branch: 7,
        })
    );
}

#[test]
fn normal_save_preserves_current_device_management_and_visibility() {
    let (config, windows_id, _) = dual_device_config();
    let mut owners = ConfigurationOwners::from_legacy(&config, &windows_id);
    let current = owners.device_profiles.get_mut(&windows_id).unwrap();
    current.games.get_mut("example-game").unwrap().visible = false;
    let mut edited = owners.assemble_effective().unwrap();

    owners.merge_effective(&edited).unwrap();
    assert!(!owners.device_profiles[&windows_id].games["example-game"].visible);

    owners
        .device_profiles
        .get_mut(&windows_id)
        .unwrap()
        .games
        .remove("example-game");
    edited.settings.prompt_when_not_described = !edited.settings.prompt_when_not_described;
    owners.merge_effective(&edited).unwrap();
    assert!(
        !owners.device_profiles[&windows_id]
            .games
            .contains_key("example-game")
    );
}

#[test]
fn normal_save_removes_an_explicitly_deleted_device_profile() {
    let (config, windows_id, deck_id) = dual_device_config();
    let mut owners = ConfigurationOwners::from_legacy(&config, &windows_id);
    let mut edited = owners.assemble_effective().unwrap();
    edited.devices.remove(&deck_id);
    edited.games[0].game_paths.remove(&deck_id);
    if let crate::backup::SaveUnitSource::Concrete { paths, .. } =
        &mut edited.games[0].save_paths[0].source
    {
        paths.remove(&deck_id);
    }

    owners.merge_effective(&edited).unwrap();

    assert!(!owners.device_profiles.contains_key(&deck_id));
}

#[test]
fn validation_rejects_unsupported_owner_schema() {
    let (config, windows_id, _) = dual_device_config();
    let mut owners = ConfigurationOwners::from_legacy(&config, &windows_id);
    owners.shared_library.schema_version = 99;

    let result = owners.validate();

    assert!(matches!(
        result,
        Err(OwnershipError::UnsupportedSchema { found: 99, .. })
    ));
}
