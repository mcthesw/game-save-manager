use std::collections::HashMap;

use super::{Config, ConfigurationOwners, OwnershipError, QuickActionsSettings};
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
