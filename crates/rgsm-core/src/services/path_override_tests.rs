use super::*;
use serde_json::json;

#[test]
fn missing_explicit_location_blocks_partial_backup() {
    let temp = temp_dir::TempDir::new().unwrap();
    let present = temp.path().join("present.sav");
    std::fs::write(&present, b"save").unwrap();
    let device = get_current_device_id();
    let service = ServiceContext::new(std::sync::Arc::new(crate::hooks::HookPipeline::new(vec![])));
    let mut game: Game = serde_json::from_value(json!({
        "name": "Required paths", "save_paths": [
            {"id": 1, "source": {"type": "devicePaths", "unit_type": "File", "paths": {device: present}}},
            {"id": 2, "source": {"type": "devicePaths", "unit_type": "File", "paths": {device: temp.path().join("missing/*.sav")}}}
        ]
    })).unwrap();
    assert!(service.capture_plan(&Config::default(), &game).is_err());
    game.save_paths[1].source =
        serde_json::from_value(json!({"type": "manifestPattern", "pattern": present})).unwrap();
    game.device_bindings = serde_json::from_value(
        json!({device: {"pathOverrides": {"2": {"expression": temp.path().join("missing.sav")}}}}),
    )
    .unwrap();
    assert!(service.capture_plan(&Config::default(), &game).is_err());
}

#[test]
fn scoped_variables_restore_all_slots_to_target_device_account() {
    use crate::backup::{ArchiveBackend, CompressionPreset, RestorePlan, SevenZBackend};
    let temp = temp_dir::TempDir::new().unwrap();
    let device_id = get_current_device_id();
    let mut config = Config::default();
    let source = temp.path().join("source");
    std::fs::create_dir_all(source.join("game-account")).unwrap();
    for slot in ["slot1.sav", "slot2.sav"] {
        std::fs::write(source.join("game-account").join(slot), slot.as_bytes()).unwrap();
    }
    let mut device: crate::device::Device = serde_json::from_value(json!({
        "id": device_id, "name": "Device", "path_variables": {"root": source, "account": "device-account"}
    })).unwrap();
    config.devices.insert(device_id.clone(), device.clone());
    let mut game: Game = serde_json::from_value(json!({
        "name": "Variables", "save_paths": [{"id": 7, "source": {
            "type": "manifestPattern", "pattern": "<var:root>/<var:account>/*.sav", "expected_type": "File"
        }}], "device_bindings": {device_id: {"pathVariables": {"account": "game-account"}}}
    })).unwrap();
    let service = ServiceContext::new(std::sync::Arc::new(crate::hooks::HookPipeline::new(vec![])));
    let capture = service.capture_plan(&config, &game).unwrap();
    assert_eq!(capture.groups.len(), 2);
    let archive = temp.path().join("save.7z");
    SevenZBackend
        .compress_capture_plan(&capture, &archive, CompressionPreset::Standard, None)
        .unwrap();
    let target = temp.path().join("target");
    device = serde_json::from_value(json!({"id": device_id, "name": "Target", "path_variables": {"root": target, "account": "target-account"}})).unwrap();
    config.devices.insert(device_id.clone(), device);
    game.device_bindings.clear();
    let report = service.resolve_save_unit_for_restore(&config, &game, &game.save_paths[0]);
    let manifest = SevenZBackend.read_capture_manifest(&archive).unwrap();
    let plan = RestorePlan::build(&manifest.groups, &[(7, report)].into_iter().collect()).unwrap();
    SevenZBackend.restore_capture_plan(&plan, &archive).unwrap();
    for slot in ["slot1.sav", "slot2.sav"] {
        assert_eq!(
            std::fs::read(target.join("target-account").join(slot)).unwrap(),
            slot.as_bytes()
        );
    }
    assert!(!target.join("game-account").exists());
}

#[test]
fn legacy_bracket_paths_upgrade_once_and_keep_literal_matches() {
    let temp = temp_dir::TempDir::new().unwrap();
    let path = temp.path().join("Game[One]");
    std::fs::create_dir(&path).unwrap();
    let device = get_current_device_id();
    let game: Game = serde_json::from_value(json!({
        "name": "Legacy brackets",
        "save_paths": [{"source": {"type": "concrete", "unit_type": "Folder", "paths": {device: path}}}]
    })).unwrap();
    let reloaded: Game = serde_json::from_value(serde_json::to_value(&game).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&game.save_paths).unwrap(),
        serde_json::to_value(&reloaded.save_paths).unwrap()
    );
    let service = ServiceContext::new(std::sync::Arc::new(crate::hooks::HookPipeline::new(vec![])));
    let report = service.resolve_save_unit(&Config::default(), &reloaded, &reloaded.save_paths[0]);
    assert_eq!(PathBuf::from(&report.locations[0].path), path);
}

#[test]
fn manual_glob_preview_and_capture_include_every_match() {
    let temp = temp_dir::TempDir::new().unwrap();
    for account in ["111", "222"] {
        std::fs::create_dir_all(temp.path().join(account).join("SaveGames")).unwrap();
    }
    let path = format!(
        "{}/*/SaveGames",
        temp.path().to_string_lossy().replace('\\', "/")
    );
    let device = get_current_device_id();
    let game: Game = serde_json::from_value(json!({
        "name": "Manual glob",
        "save_paths": [{"id": 1, "source": {"type": "concrete", "unit_type": "Folder", "paths": {device: path}}}]
    })).unwrap();
    let service = ServiceContext::new(std::sync::Arc::new(crate::hooks::HookPipeline::new(vec![])));
    let config = Config::default();
    let preview = service.check_ad_hoc_paths(
        &config,
        std::slice::from_ref(&path),
        &PathPreviewContext::default(),
    );
    assert!(matches!(
        preview[0],
        crate::path_resolver::PathCheckResult::Ok { .. }
    ));
    let capture = service.capture_plan(&config, &game).unwrap();
    assert_eq!(capture.groups.len(), 2);
    assert_eq!(capture.groups[0].relative_path, "111/SaveGames");
    assert_eq!(capture.groups[1].relative_path, "222/SaveGames");
}

#[test]
fn device_override_replaces_pattern_without_fallback() {
    let temp = temp_dir::TempDir::new().unwrap();
    let original = temp.path().join("original.sav");
    let local = temp.path().join("local.sav");
    std::fs::write(&original, b"original").unwrap();
    std::fs::write(&local, b"local").unwrap();
    let device = get_current_device_id();
    let mut game: Game = serde_json::from_value(json!({
        "name": "Override test",
        "save_paths": [{"id": 1, "source": {"type": "manifestPattern", "pattern": original}}],
        "device_bindings": {device: {"pathOverrides": {"1": {"path": local, "unitType": "File"}}}}
    }))
    .unwrap();
    let service = ServiceContext::new(std::sync::Arc::new(crate::hooks::HookPipeline::new(vec![])));
    let config = Config::default();
    let report = service.resolve_save_unit(&config, &game, &game.save_paths[0]);
    assert_eq!(PathBuf::from(&report.locations[0].path), local);
    std::fs::remove_file(&local).unwrap();
    let report = service.resolve_save_unit(&config, &game, &game.save_paths[0]);
    assert!(
        report.locations.is_empty(),
        "missing override must not use the original save"
    );
    let report = service.resolve_save_unit_for_restore(&config, &game, &game.save_paths[0]);
    assert_eq!(report.candidates[0].exact_target_path(), Some(local));
    let binding = game.device_bindings.remove(device).unwrap();
    game.device_bindings
        .insert("another-device".into(), binding);
    let report = service.resolve_save_unit(&config, &game, &game.save_paths[0]);
    assert_eq!(PathBuf::from(&report.locations[0].path), original);
}

#[test]
fn override_cannot_change_a_declared_file_into_a_folder() {
    let temp = temp_dir::TempDir::new().unwrap();
    let device = get_current_device_id();
    let game: Game = serde_json::from_value(json!({
        "name": "Fixed type",
        "save_paths": [{"id": 1, "source": {
            "type": "manifestPattern", "pattern": "original.sav", "expected_type": "File"
        }}],
        "device_bindings": {device: {"pathOverrides": {"1": {
            "path": temp.path(), "unitType": "Folder"
        }}}}
    }))
    .unwrap();
    let service = ServiceContext::new(std::sync::Arc::new(crate::hooks::HookPipeline::new(vec![])));
    let report = service.resolve_save_unit(&Config::default(), &game, &game.save_paths[0]);
    assert!(
        report.locations.is_empty(),
        "an override must preserve the declared file type"
    );
}

#[test]
fn variables_after_globs_restore_each_slot_without_old_account_names() {
    use crate::backup::{
        ArchiveBackend, ArchiveVersion, CompressionPreset, RestorePlan, SevenZBackend,
    };
    let temp = temp_dir::TempDir::new().unwrap();
    let id = get_current_device_id();
    let mut config = Config::default();
    let source = temp.path().join("source");
    for slot in ["slot1", "slot2"] {
        std::fs::create_dir_all(source.join(slot).join("account[old]")).unwrap();
        std::fs::write(
            source.join(slot).join("account[old]/save.sav"),
            slot.as_bytes(),
        )
        .unwrap();
    }
    config.devices.insert(id.clone(), serde_json::from_value(json!({"id": id, "name": "Source", "path_variables": {"root": source, "account": "account[old]"}})).unwrap());
    let game: Game = serde_json::from_value(json!({"name": "Slots", "save_paths": [{"id": 7, "source": {"type": "manifestPattern", "pattern": "<var:root>/*/<var:account>/*.sav"}}]})).unwrap();
    let service = ServiceContext::new(std::sync::Arc::new(crate::hooks::HookPipeline::new(vec![])));
    let capture = service.capture_plan(&config, &game).unwrap();
    assert_eq!(capture.groups.len(), 2);
    assert!(
        crate::backup::ZipBackend
            .compress_capture_plan(
                &capture,
                &temp.path().join("unsupported.zip"),
                CompressionPreset::Standard,
                None
            )
            .is_err()
    );
    assert_eq!(
        capture.groups[0].relative_expression.as_deref(),
        Some("slot1/<var:account>/save.sav")
    );
    let archive = temp.path().join("save.7z");
    SevenZBackend
        .compress_capture_plan(&capture, &archive, CompressionPreset::Standard, None)
        .unwrap();
    assert_eq!(
        SevenZBackend.archive_version(&archive).unwrap(),
        ArchiveVersion::V5
    );
    let target = temp.path().join("target");
    config.devices.insert(id.clone(), serde_json::from_value(json!({"id": id, "name": "Target", "path_variables": {"root": target, "account": "new-account"}})).unwrap());
    let report = service.resolve_save_unit_for_restore(&config, &game, &game.save_paths[0]);
    let manifest = SevenZBackend.read_capture_manifest(&archive).unwrap();
    let plan = RestorePlan::build(&manifest.groups, &[(7, report)].into_iter().collect()).unwrap();
    SevenZBackend.restore_capture_plan(&plan, &archive).unwrap();
    for slot in ["slot1", "slot2"] {
        assert_eq!(
            std::fs::read(target.join(slot).join("new-account/save.sav")).unwrap(),
            slot.as_bytes()
        );
        assert!(!target.join(slot).join("account[old]").exists());
    }
}
