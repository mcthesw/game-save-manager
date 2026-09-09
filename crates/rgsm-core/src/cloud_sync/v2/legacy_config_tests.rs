use super::{CloudNamespaceClassification, CloudNamespaceClassifier};
use crate::{cloud_sync::V1_CONFIG_PATH, config::ConfigurationOwners};
use opendal::{Operator, services};

const RELEASED_18: &str = include_str!("../../../tests/fixtures/config-upgrade/config_v1_8_0.json");

#[test]
fn legacy_cloud_identity_preserves_directory_names_and_existing_keys() {
    let mut raw: serde_json::Value = serde_json::from_str(RELEASED_18).unwrap();
    raw["games"][0]["name"] = "A__B".into();
    let config = crate::updater::decode_legacy_cloud_config(&raw.to_string()).unwrap();
    assert_eq!(config.games[0].storage_key, "A__B");

    raw["games"][0]["storage_key"] = "existing-folder".into();
    let config = crate::updater::decode_legacy_cloud_config(&raw.to_string()).unwrap();
    assert_eq!(config.games[0].storage_key, "existing-folder");

    raw["version"] = "1.9.0".into();
    raw["games"][0]["storage_key"] = "".into();
    let config = crate::updater::decode_legacy_cloud_config(&raw.to_string()).unwrap();
    assert!(
        config.games[0].storage_key.is_empty(),
        "do not invent identities for invalid current-format data"
    );
}

#[test]
fn legacy_cloud_schema_preserves_declared_types_and_explicit_root_bindings() {
    let mut raw: serde_json::Value = serde_json::from_str(RELEASED_18).unwrap();
    raw["games"][0]["save_paths"][1]["paths"]["desktop-gamma"] = "<root>/Saved".into();
    raw["devices"]["desktop-gamma"]["game_roots"] = serde_json::json!(["D:/Games"]);
    let config = crate::updater::decode_legacy_cloud_config(&raw.to_string()).unwrap();
    let game = &config.games[0];
    assert_eq!(
        game.save_paths[0].unit_type(),
        Some(&crate::backup::SaveUnitType::WinRegistry)
    );
    assert_eq!(
        game.save_paths[1].unit_type(),
        Some(&crate::backup::SaveUnitType::Folder)
    );
    assert!(matches!(
        &game.save_paths[1].source,
        crate::backup::SaveUnitSource::ManifestPattern { .. }
    ));
    let root_id = game.device_bindings["desktop-gamma"]
        .root_ids
        .as_ref()
        .unwrap()[0];
    assert!(matches!(
        &config.devices["desktop-gamma"].resource(root_id).unwrap().kind,
        crate::device::DeviceResourceKind::GameRoot { path, .. } if path == "D:/Games"
    ));
}

#[tokio::test]
async fn released_cloud_configs_have_usable_game_identities_without_remote_writes() {
    for bytes in [
        include_bytes!("../../../tests/fixtures/config-upgrade/config_v1_7_0.json").as_slice(),
        include_bytes!("../../../tests/fixtures/config-upgrade/config_v1_8_0.json").as_slice(),
    ] {
        let op = Operator::new(services::Memory::default()).unwrap().finish();
        op.write(V1_CONFIG_PATH, bytes.to_vec()).await.unwrap();
        let CloudNamespaceClassification::V1Only { config } =
            CloudNamespaceClassifier::new(op.clone())
                .classify()
                .await
                .unwrap()
        else {
            panic!("released configuration should remain in the V1 namespace");
        };
        for game in &config.games {
            assert_eq!(game.storage_key, game.name);
            assert!(
                game.next_save_unit_id > game.save_paths.iter().map(|unit| unit.id).max().unwrap()
            );
        }
        ConfigurationOwners::from_legacy(&config, &"reader".into())
            .validate()
            .unwrap();
        assert_eq!(op.read(V1_CONFIG_PATH).await.unwrap().to_vec(), bytes);
        assert!(op.list("v2/").await.unwrap().is_empty());
    }
}
