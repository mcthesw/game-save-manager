use super::*;
use serde_json::json;

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
