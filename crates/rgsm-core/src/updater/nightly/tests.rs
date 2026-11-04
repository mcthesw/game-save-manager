use crate::backup::{Game, SaveUnit, SaveUnitType};
use crate::config::Config;
use crate::device::{Device, DeviceResourceKind};
use crate::path_pattern::StoreKind;

use super::*;

fn device(id: &str) -> Device {
    Device {
        id: id.to_string(),
        name: "Test".to_string(),
        resources: Vec::new(),
        path_variables: Default::default(),
        next_resource_id: 0,
    }
}

#[test]
fn released_versions_are_never_mistaken_for_nightly_configs() {
    for fixture in [
        include_str!("../../../tests/fixtures/config-upgrade/config_v1_0_0.json"),
        include_str!("../../../tests/fixtures/config-upgrade/config_v1_4_0.json"),
        include_str!("../../../tests/fixtures/config-upgrade/config_v1_5_6.json"),
        include_str!("../../../tests/fixtures/config-upgrade/config_v1_7_0.json"),
        include_str!("../../../tests/fixtures/config-upgrade/config_v1_8_0.json"),
    ] {
        let raw: Value = serde_json::from_str(fixture).unwrap();
        assert!(!detect(&raw));
    }
}

#[test]
fn each_nightly_field_is_detected() {
    for raw in [
        serde_json::json!({"devices": {"d": {"game_roots": []}}}),
        serde_json::json!({"games": [{"store_user_ids": {}}]}),
        serde_json::json!({"games": [{"ludusavi_meta": {"steamId": 1}}]}),
        serde_json::json!({"games": [{"ludusavi_meta": {"steam_id": 1}}]}),
    ] {
        assert!(detect(&raw), "{raw}");
    }
}

#[test]
fn verified_current_device_root_migrates_as_steam() {
    let mut config = Config::default();
    config
        .devices
        .insert("device".to_string(), device("device"));
    let raw = serde_json::json!({
        "devices": {
            "device": {
                "game_roots": ["D:\\SteamLibrary"]
            }
        },
        "games": []
    });

    apply(
        &raw,
        &mut config,
        Some("device"),
        &["d:/steamlibrary/".to_string()],
    );

    assert!(matches!(
        config.devices["device"].resources[0].kind,
        DeviceResourceKind::GameRoot {
            store: StoreKind::Steam,
            ..
        }
    ));
}

#[test]
fn multiple_legacy_roots_remain_unbound_for_explicit_player_selection() {
    let device_id = "device".to_string();
    let mut config = Config::default();
    config.devices.insert(device_id.clone(), device(&device_id));
    config.games.push(Game {
        name: "Legacy".to_string(),
        storage_key: "legacy".to_string(),
        save_paths: vec![SaveUnit::concrete(
            7,
            SaveUnitType::Folder,
            std::collections::HashMap::from([(device_id.clone(), "<root>/Saved".to_string())]),
            false,
            true,
        )],
        game_paths: Default::default(),
        next_save_unit_id: 8,
        cloud_sync_enabled: false,
        auto_backup: None,
        auto_backup_limit: None,
        ludusavi_meta: None,
        device_bindings: Default::default(),
    });
    let raw = serde_json::json!({
        "devices": {
            "device": { "game_roots": ["D:/Games", "E:/Games"] }
        },
        "games": [{
            "save_paths": [{
                "id": 7,
                "unit_type": "Folder",
                "paths": { "device": "<root>/Saved" }
            }]
        }]
    });

    apply(&raw, &mut config, Some(&device_id), &[]);

    assert!(!config.games[0].device_bindings.contains_key(&device_id));
}
