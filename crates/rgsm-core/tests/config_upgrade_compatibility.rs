//! Executable configuration-upgrade compatibility contract.
//!
//! Keep one realistic fixture for every materially different schema shipped in a release
//! between `MIN_SUPPORTED_VERSION` and the latest release tag. Releases that serialize the
//! same shape share a fixture; unreleased schemas do not define compatibility promises.

use std::collections::HashMap;
use std::fs;
use std::io::Write;

use serde_json::Value;
use zip::{ZipWriter, write::SimpleFileOptions};

use rgsm_core::backup::{
    ArchiveBackend, ArchiveFormat, CreatedBy, GameSnapshots, SaveUnit, SaveUnitSource,
    SaveUnitType, ZipBackend,
};
use rgsm_core::config::Config;
use rgsm_core::device::get_current_device_id;
use rgsm_core::preclude::UpdaterError;
use rgsm_core::updater::migration::update_config;
use rgsm_core::updater::versions::{CURRENT_VERSION, MIN_SUPPORTED_VERSION};

const LEGACY_SINGLE_DEVICE: &str = include_str!("fixtures/config-upgrade/config_v1_0_0.json");
const LEGACY_SINGLE_DEVICE_WITH_REFERENCES: &str =
    include_str!("fixtures/config-upgrade/config_v1_4_0.json");
const DEVICE_KEYED: &str = include_str!("fixtures/config-upgrade/config_v1_5_6.json");
const STABLE_SAVE_UNIT_IDS: &str = include_str!("fixtures/config-upgrade/config_v1_8_0.json");
const RELEASED_SNAPSHOT_DATE: &str = "2025-01-02_03-04-05";
const RELEASED_SAVE_CONTENT: &[u8] = b"released-save-content";

#[derive(Clone, Copy)]
enum ReleasedSaveData {
    V1_0FlatFile,
    V1_5FlatFolder,
    V1_8IdPrefixedFolder,
}

impl ReleasedSaveData {
    fn save_unit_index(self) -> usize {
        match self {
            Self::V1_0FlatFile | Self::V1_5FlatFolder => 0,
            Self::V1_8IdPrefixedFolder => 1,
        }
    }

    fn target_name(self) -> &'static str {
        match self {
            Self::V1_0FlatFile => "slot1.sav",
            Self::V1_5FlatFolder => "Saves",
            Self::V1_8IdPrefixedFolder => "Saved",
        }
    }

    fn archive_entry(self) -> &'static str {
        match self {
            Self::V1_0FlatFile => "slot1.sav",
            Self::V1_5FlatFolder => "Saves/profile.sav",
            Self::V1_8IdPrefixedFolder => "11/Saved/profile.sav",
        }
    }

    fn compression_method(self) -> zip::CompressionMethod {
        match self {
            Self::V1_0FlatFile => zip::CompressionMethod::Stored,
            Self::V1_5FlatFolder => zip::CompressionMethod::Bzip2,
            Self::V1_8IdPrefixedFolder => zip::CompressionMethod::Zstd,
        }
    }

    fn archive_comment(self) -> Option<&'static str> {
        match self {
            Self::V1_0FlatFile | Self::V1_5FlatFolder => None,
            Self::V1_8IdPrefixedFolder => {
                Some("RGSM_ARCHIVE_V2\n{\"version\":2,\"compression\":\"zstd:3\"}")
            }
        }
    }

    fn has_released_device_heads(self) -> bool {
        matches!(self, Self::V1_8IdPrefixedFolder)
    }
}

struct MigratedFixture {
    config: Config,
}

fn seed_released_save_data(
    backup_path: &std::path::Path,
    game_name: &str,
    shape: ReleasedSaveData,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let game_dir = backup_path.join(game_name);
    fs::create_dir_all(&game_dir)?;
    let archive_path = game_dir.join(format!("{RELEASED_SNAPSHOT_DATE}.zip"));
    let mut writer = ZipWriter::new(fs::File::create(&archive_path)?);
    if let Some(comment) = shape.archive_comment() {
        writer.set_comment(comment);
    }
    writer.start_file(
        shape.archive_entry(),
        SimpleFileOptions::default().compression_method(shape.compression_method()),
    )?;
    writer.write_all(RELEASED_SAVE_CONTENT)?;
    writer.finish()?;

    let snapshot = serde_json::json!({
        "date": RELEASED_SNAPSHOT_DATE,
        "describe": "Released snapshot fixture",
        "path": format!("save_data/{game_name}/{RELEASED_SNAPSHOT_DATE}.zip"),
        "size": fs::metadata(&archive_path)?.len(),
        "device_id": shape.has_released_device_heads().then_some("released-device")
    });
    let metadata = if shape.has_released_device_heads() {
        serde_json::json!({
            "name": game_name,
            "backups": [snapshot],
            "device_heads": {"released-device": RELEASED_SNAPSHOT_DATE},
            "sync_version": 3,
            "last_sync_device": "released-device"
        })
    } else {
        serde_json::json!({
            "name": game_name,
            "backups": [snapshot]
        })
    };
    fs::write(
        game_dir.join("Backups.json"),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    Ok(fs::read(archive_path)?)
}

fn assert_released_save_data_survives_upgrade(
    config: &Config,
    backup_path: &std::path::Path,
    shape: ReleasedSaveData,
    original_archive: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let game = &config.games[0];
    let game_dir = backup_path.join(game.backup_dir_name().as_ref());
    let archive_path = game_dir.join(format!("{RELEASED_SNAPSHOT_DATE}.zip"));
    assert_eq!(fs::read(&archive_path)?, original_archive);

    let mut snapshots: GameSnapshots =
        serde_json::from_slice(&fs::read(game_dir.join("Backups.json"))?)?;
    snapshots.normalize_heads();
    assert_eq!(snapshots.backups.len(), 1);
    assert_eq!(snapshots.backups[0].date, RELEASED_SNAPSHOT_DATE);
    assert_eq!(snapshots.backups[0].archive_format, ArchiveFormat::Zip);
    assert_eq!(snapshots.backups[0].created_by, CreatedBy::Manual);
    assert!(
        snapshots
            .head_entries()
            .any(|(_, head)| head == RELEASED_SNAPSHOT_DATE)
    );

    let target = backup_path.join("restored").join(shape.target_name());
    let mut save_unit = game.save_paths[shape.save_unit_index()].clone();
    let SaveUnitSource::Concrete { paths, .. } = &mut save_unit.source else {
        panic!("released fixture must migrate to a concrete Save Unit");
    };
    paths.clear();
    paths.insert(
        get_current_device_id().clone(),
        target.to_string_lossy().into_owned(),
    );
    ZipBackend.decompress(std::slice::from_ref(&save_unit), &archive_path, None, None)?;

    let restored_file = match save_unit.unit_type() {
        Some(SaveUnitType::File) => target,
        Some(SaveUnitType::Folder) => target.join("profile.sav"),
        _ => panic!("released fixture must use a filesystem Save Unit"),
    };
    assert_eq!(fs::read(restored_file)?, RELEASED_SAVE_CONTENT);
    Ok(())
}

fn migrate_fixture(
    fixture: &str,
    save_data_shape: ReleasedSaveData,
) -> Result<MigratedFixture, Box<dyn std::error::Error>> {
    let temp_dir = temp_dir::TempDir::new()?;
    let config_path = temp_dir.path().join("GameSaveManager.config.json");
    let backup_path = temp_dir.path().join("save_data");
    let mut raw: Value = serde_json::from_str(fixture)?;
    raw["backup_path"] = Value::String(backup_path.to_string_lossy().into_owned());
    let game_name = raw
        .pointer("/games/0/name")
        .and_then(Value::as_str)
        .unwrap();
    let original_archive = seed_released_save_data(&backup_path, game_name, save_data_shape)?;
    fs::write(&config_path, serde_json::to_string_pretty(&raw)?)?;

    assert!(
        update_config(&config_path)?,
        "fixture should require migration"
    );
    assert!(
        config_path.with_extension("json.bak").is_file(),
        "migration should preserve the original config"
    );

    let migrated_text = fs::read_to_string(&config_path)?;
    let config: Config = serde_json::from_str(&migrated_text)?;
    assert_eq!(config.version, CURRENT_VERSION);
    assert_released_save_data_survives_upgrade(
        &config,
        &backup_path,
        save_data_shape,
        &original_archive,
    )?;
    assert!(
        !update_config(&config_path)?,
        "migrated fixture should be idempotent"
    );

    Ok(MigratedFixture { config })
}

fn assert_concrete_unit(
    unit: &SaveUnit,
    id: u32,
    unit_type: SaveUnitType,
    paths: &[(&str, &str)],
    delete_before_apply: bool,
    enabled: bool,
) {
    assert_eq!(unit.id, id);
    assert_eq!(unit.unit_type(), Some(&unit_type));
    assert_eq!(unit.delete_before_apply, delete_before_apply);
    assert_eq!(unit.enabled, enabled);
    let expected = paths
        .iter()
        .map(|(device, path)| ((*device).to_string(), (*path).to_string()))
        .collect::<HashMap<_, _>>();
    assert_eq!(unit.paths(), Some(&expected));
}

#[test]
fn oldest_supported_single_device_config_upgrades_without_player_data_loss()
-> Result<(), Box<dyn std::error::Error>> {
    let migrated = migrate_fixture(LEGACY_SINGLE_DEVICE, ReleasedSaveData::V1_0FlatFile)?;
    let config = migrated.config;
    assert_eq!(config.games.len(), 1);
    assert_eq!(config.devices.len(), 1);

    let device_id = config.devices.keys().next().unwrap();
    let game = &config.games[0];
    assert_eq!(game.name, "Legacy Adventure");
    assert!(!game.storage_key.is_empty());
    assert_eq!(game.next_save_unit_id, 2);
    assert_eq!(
        game.game_paths.get(device_id).map(String::as_str),
        Some("D:/Games/Legacy Adventure/legacy.exe")
    );
    assert_concrete_unit(
        &game.save_paths[0],
        0,
        SaveUnitType::File,
        &[(
            device_id,
            "C:/Users/Player/Saved Games/Legacy Adventure/slot1.sav",
        )],
        true,
        true,
    );
    assert_concrete_unit(
        &game.save_paths[1],
        1,
        SaveUnitType::Folder,
        &[(
            device_id,
            "C:/Users/Player/AppData/Local/Legacy Adventure/Saves",
        )],
        false,
        true,
    );
    assert!(!config.settings.extra_backup_when_apply);
    assert!(!config.settings.default_delete_before_apply);
    assert_eq!(config.settings.locale, "zh_SIMPLIFIED");

    Ok(())
}

#[test]
fn version_1_4_config_preserves_favorites_and_selected_quick_action_game()
-> Result<(), Box<dyn std::error::Error>> {
    let migrated = migrate_fixture(
        LEGACY_SINGLE_DEVICE_WITH_REFERENCES,
        ReleasedSaveData::V1_0FlatFile,
    )?;
    let config = migrated.config;
    let device_id = config.devices.keys().next().unwrap();
    let game = &config.games[0];
    let selected = config.selected_quick_action_game().unwrap();

    assert_eq!(selected.storage_key, game.storage_key);
    assert_eq!(selected.name, game.name);
    assert_eq!(
        selected.game_paths.get(device_id),
        game.game_paths.get(device_id)
    );

    let raw = serde_json::to_value(&config)?;
    assert_eq!(
        raw.pointer("/favorites/0/label"),
        Some(&Value::String("RPG".into()))
    );
    assert_eq!(
        raw.pointer("/quick_action/quick_action_game_id"),
        raw.pointer("/games/0/storage_key")
    );
    Ok(())
}

#[test]
fn device_keyed_config_upgrades_and_preserves_multi_device_save_units()
-> Result<(), Box<dyn std::error::Error>> {
    let migrated = migrate_fixture(DEVICE_KEYED, ReleasedSaveData::V1_5FlatFolder)?;
    let config = migrated.config;
    assert_eq!(config.devices.len(), 2);

    let game = &config.games[0];
    assert_eq!(game.name, "Cross Device Quest");
    assert!(game.cloud_sync_enabled);
    assert_eq!(game.next_save_unit_id, 2);
    assert_eq!(
        game.game_paths.get("desktop-alpha").map(String::as_str),
        Some("D:/Games/Cross Device Quest/game.exe")
    );
    assert_eq!(
        game.game_paths.get("deck-beta").map(String::as_str),
        Some("/home/deck/Games/cross-device-quest/game")
    );
    assert_concrete_unit(
        &game.save_paths[0],
        0,
        SaveUnitType::Folder,
        &[
            (
                "desktop-alpha",
                "C:/Users/Player/Documents/My Games/Cross Device Quest/Saves",
            ),
            (
                "deck-beta",
                "/home/deck/.local/share/Cross Device Quest/Saves",
            ),
        ],
        true,
        true,
    );
    assert_concrete_unit(
        &game.save_paths[1],
        1,
        SaveUnitType::File,
        &[
            (
                "desktop-alpha",
                "C:/Users/Player/AppData/Local/CDQ/settings.json",
            ),
            ("deck-beta", "/home/deck/.config/cdq/settings.json"),
        ],
        false,
        true,
    );
    assert!(config.settings.save_list_last_expanded);
    assert_eq!(config.settings.max_auto_backup_count, 7);
    assert_eq!(config.settings.cloud_settings.max_concurrency, 1);
    Ok(())
}

#[test]
fn stable_id_config_preserves_ids_and_non_default_settings()
-> Result<(), Box<dyn std::error::Error>> {
    let migrated = migrate_fixture(STABLE_SAVE_UNIT_IDS, ReleasedSaveData::V1_8IdPrefixedFolder)?;
    let config = migrated.config;
    let game = &config.games[0];

    assert_eq!(game.name, "Registry & Files");
    assert!(!game.cloud_sync_enabled);
    assert_eq!(game.next_save_unit_id, 12);
    assert_concrete_unit(
        &game.save_paths[0],
        7,
        SaveUnitType::WinRegistry,
        &[(
            "desktop-gamma",
            "HKEY_CURRENT_USER\\Software\\Example Studio\\Registry & Files",
        )],
        false,
        true,
    );
    assert_concrete_unit(
        &game.save_paths[1],
        11,
        SaveUnitType::Folder,
        &[(
            "desktop-gamma",
            "C:/Users/Player/AppData/Local/Registry & Files/Saved",
        )],
        true,
        true,
    );
    assert_eq!(config.settings.max_extra_backup_count, 9);
    assert!(config.settings.compute_archive_hash);
    assert!(config.settings.verify_archive_before_apply);
    assert_eq!(config.settings.appearance.ui_font_family, "Noto Sans");
    Ok(())
}

#[test]
fn direct_upgrade_contract_rejects_versions_outside_the_supported_range()
-> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = temp_dir::TempDir::new()?;
    let config_path = temp_dir.path().join("GameSaveManager.config.json");

    fs::write(&config_path, r#"{"version":"0.9.9"}"#)?;
    let error = update_config(&config_path).unwrap_err();
    assert!(matches!(
        error,
        UpdaterError::ConfigVersionTooOld { ref found, ref min_supported, .. }
            if found.to_string() == "0.9.9" && min_supported.to_string() == MIN_SUPPORTED_VERSION
    ));

    fs::write(&config_path, r#"{"version":"999.0.0"}"#)?;
    let error = update_config(&config_path).unwrap_err();
    assert!(matches!(
        error,
        UpdaterError::ConfigVersionTooNew { ref found, ref current, .. }
            if found.to_string() == "999.0.0" && current.to_string() == CURRENT_VERSION
    ));

    fs::write(&config_path, r#"{"backup_path":"save_data"}"#)?;
    assert!(matches!(
        update_config(&config_path).unwrap_err(),
        UpdaterError::MissingVersion { .. }
    ));

    fs::write(
        &config_path,
        r#"{
            "version": "1.5.6",
            "backup_path": "save_data",
            "games": [{"name": "Broken", "save_paths": [{}]}],
            "settings": {}
        }"#,
    )?;
    assert!(matches!(
        update_config(&config_path).unwrap_err(),
        UpdaterError::Deserialize(_)
    ));
    Ok(())
}
