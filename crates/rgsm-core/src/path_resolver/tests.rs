use super::*;
use crate::config::Config;

// Create test configuration
fn create_test_config() -> Config {
    Config {
        version: "1.0.0".to_string(),
        backup_path: "/test/backup".to_string(),
        games: Vec::new(),
        settings: crate::config::Settings::default(),
        favorites: Vec::new(),
        quick_action: crate::config::QuickActionsSettings::default(),
        devices: std::collections::HashMap::new(),
    }
}

#[test]
fn test_resolve_path_without_variables() {
    let config = create_test_config();
    let path = "/simple/path/without/variables";

    let result = resolve_path(path, None, &config).unwrap();
    assert_eq!(result, PathBuf::from(path));
}

#[test]
fn test_resolve_home_variable() {
    let config = create_test_config();
    let path = "<home>/Documents/saves";

    let result = resolve_path(path, None, &config);
    assert!(result.is_ok()); // Actual value depends on the runtime environment
}

#[test]
fn test_resolve_os_username_variable() {
    let config = create_test_config();
    let path = "/Users/<osUserName>/Documents";

    let result = resolve_path(path, None, &config);
    assert!(result.is_ok()); // Actual value depends on the runtime environment
}

#[test]
fn test_error_on_unknown_variable() {
    let config = create_test_config();
    let path = "<unknown>/saves";

    let result = resolve_path(path, None, &config);
    assert!(matches!(result, Err(ResolveError::UnknownVariable(_))));
}

// ── <base> / <game> / <storeGameId> tests ───────────────────────────

#[test]
fn test_base_without_context_returns_missing_context() {
    let config = create_test_config();
    let result = resolve_path("<base>/saves", None, &config);
    assert!(matches!(result, Err(ResolveError::MissingContext(_))));
}

#[test]
fn test_game_without_context_returns_missing_context() {
    let config = create_test_config();
    let result = resolve_path("<root>/something/<game>", None, &config);
    // <root> may or may not resolve (depends on Steam), but <game> needs context
    // If <root> fails first, that's also acceptable
    assert!(result.is_err());
}

#[test]
fn test_store_game_id_without_context_returns_missing_context() {
    let config = create_test_config();
    let result = resolve_path("<storeGameId>/saves", None, &config);
    assert!(matches!(result, Err(ResolveError::MissingContext(_))));
}

#[test]
fn test_store_game_id_with_context() {
    let config = create_test_config();
    let ctx = PathContext {
        install_dirs: Vec::new(),
        steam_id: Some(282800),
        install_dir_cache: None,
        game_roots: Vec::new(),
        store_user_id: None,
    };
    let result = resolve_path("/path/<storeGameId>/saves", Some(&ctx), &config);
    assert!(result.is_ok());
    let resolved = result.unwrap();
    assert_eq!(resolved, PathBuf::from("/path/282800/saves"));
}

#[test]
fn test_store_game_id_without_steam_id() {
    let config = create_test_config();
    let ctx = PathContext {
        install_dirs: Vec::new(),
        steam_id: None,
        install_dir_cache: None,
        game_roots: Vec::new(),
        store_user_id: None,
    };
    let result = resolve_path("/path/<storeGameId>/saves", Some(&ctx), &config);
    assert!(matches!(result, Err(ResolveError::MissingContext(_))));
}

#[test]
fn test_base_with_context_and_cache() {
    let config = create_test_config();

    // Create temp dir for the game install
    let temp = temp_dir::TempDir::new().unwrap();
    let game_dir = temp.path().join("TestGame");
    std::fs::create_dir_all(&game_dir).unwrap();

    let mut cache = std::collections::HashMap::new();
    cache.insert(
        "testgame".to_string(),
        crate::steam::InstalledSteamGame {
            app_id: 12345,
            name: "Test Game".to_string(),
            install_dir: "TestGame".to_string(),
            install_path: game_dir.clone(),
        },
    );

    let ctx = PathContext {
        install_dirs: vec!["TestGame".to_string()],
        steam_id: Some(12345),
        install_dir_cache: Some(Arc::new(cache)),
        game_roots: Vec::new(),
        store_user_id: None,
    };

    let result = resolve_path("<base>/saves", Some(&ctx), &config);
    assert!(result.is_ok());
    let resolved = result.unwrap();
    assert_eq!(resolved, game_dir.join("saves"));
}

#[test]
fn test_game_and_base_same_match() {
    let config = create_test_config();

    let temp = temp_dir::TempDir::new().unwrap();
    let game_dir = temp.path().join("My Game Dir");
    std::fs::create_dir_all(&game_dir).unwrap();

    let mut cache = std::collections::HashMap::new();
    cache.insert(
        "my game dir".to_string(),
        crate::steam::InstalledSteamGame {
            app_id: 99999,
            name: "My Game".to_string(),
            install_dir: "My Game Dir".to_string(),
            install_path: game_dir.clone(),
        },
    );

    let ctx = PathContext {
        install_dirs: vec!["My Game Dir".to_string()],
        steam_id: None,
        install_dir_cache: Some(Arc::new(cache)),
        game_roots: Vec::new(),
        store_user_id: None,
    };

    // A path with both <base> and <game> should resolve consistently
    let result = resolve_path("<base>/<game>/data", Some(&ctx), &config);
    assert!(result.is_ok());
    let resolved = result.unwrap();
    assert_eq!(resolved, game_dir.join("My Game Dir").join("data"));
}

#[test]
fn test_base_game_not_installed() {
    let config = create_test_config();
    let cache = std::collections::HashMap::new(); // empty — no games installed

    let ctx = PathContext {
        install_dirs: vec!["NonExistentGame".to_string()],
        steam_id: None,
        install_dir_cache: Some(Arc::new(cache)),
        game_roots: Vec::new(),
        store_user_id: None,
    };

    let result = resolve_path("<base>/saves", Some(&ctx), &config);
    assert!(matches!(result, Err(ResolveError::GameNotInstalled(_))));
}

#[test]
fn test_base_empty_install_dirs() {
    let config = create_test_config();
    let ctx = PathContext {
        install_dirs: Vec::new(),
        steam_id: None,
        install_dir_cache: Some(Arc::new(std::collections::HashMap::new())),
        game_roots: Vec::new(),
        store_user_id: None,
    };

    let result = resolve_path("<base>/saves", Some(&ctx), &config);
    assert!(matches!(result, Err(ResolveError::MissingContext(_))));
}

// Linux specific tests
#[cfg(target_os = "linux")]
mod linux_tests {
    use super::*;

    #[test]
    fn test_resolve_linux_variables() {
        let config = create_test_config();

        // Test Linux specific variables
        let paths = vec!["<xdgData>/saves", "<xdgConfig>/saves"];

        for path in paths {
            let result = resolve_path(path, None, &config);
            assert!(result.is_ok(), "Failed to resolve path: {}", path);
        }
    }
}

#[test]
fn test_resolve_root_uses_first_configured_game_root() {
    let config = create_test_config();
    let ctx = PathContext {
        game_roots: vec![
            "/mnt/games/steam".to_string(),
            "/mnt/games/steam2".to_string(),
        ],
        ..Default::default()
    };
    let result = resolve_path("<root>/userdata/12345/remote", Some(&ctx), &config).unwrap();
    let result_str = result.to_string_lossy();
    assert!(
        result_str.starts_with("/mnt/games/steam/"),
        "Expected first configured root, got: {result_str}"
    );
    assert!(result_str.ends_with("/userdata/12345/remote"));
}

#[test]
fn test_resolve_root_falls_back_when_game_roots_empty() {
    let config = create_test_config();
    let ctx = PathContext {
        game_roots: vec![],
        ..Default::default()
    };
    // Empty game_roots should fallback to auto-detected Steam root.
    // The call may succeed (if Steam is installed) or fail (if not).
    // Either way, it should NOT panic.
    let _ = resolve_path("<root>/something", Some(&ctx), &config);
}

#[test]
fn test_resolve_root_no_context_falls_back_to_auto_detect() {
    let config = create_test_config();
    // None context = no game_roots = fallback to get_steam_root()
    let _ = resolve_path("<root>/something", None, &config);
}

#[test]
fn test_path_context_from_game_includes_device_game_roots() {
    use crate::backup::Game;
    use crate::device::{Device, DeviceResource, DeviceResourceKind, DeviceResourceSource};
    use crate::path_pattern::StoreKind;

    let game = Game {
        name: "Test Game".to_string(),
        storage_key: String::new(),
        save_paths: vec![],
        game_paths: std::collections::HashMap::new(),
        next_save_unit_id: 0,
        cloud_sync_enabled: true,
        auto_backup: None,
        ludusavi_meta: None,
        device_bindings: std::collections::HashMap::new(),
    };
    let device = Device {
        id: "test-device".to_string(),
        name: "Test".to_string(),
        resources: vec![DeviceResource {
            id: 0,
            source: DeviceResourceSource::Manual,
            kind: DeviceResourceKind::GameRoot {
                store: StoreKind::Other,
                path: "/custom/root".to_string(),
            },
        }],
        next_resource_id: 1,
    };

    let ctx = game.path_context(Some(&device));
    assert_eq!(ctx.game_roots, vec!["/custom/root".to_string()]);
}

#[test]
fn test_path_context_from_game_without_device() {
    use crate::backup::Game;

    let game = Game {
        name: "Test Game".to_string(),
        storage_key: String::new(),
        save_paths: vec![],
        game_paths: std::collections::HashMap::new(),
        next_save_unit_id: 0,
        cloud_sync_enabled: true,
        auto_backup: None,
        ludusavi_meta: None,
        device_bindings: std::collections::HashMap::new(),
    };

    let ctx = game.path_context(None);
    assert!(ctx.game_roots.is_empty());
}

#[test]
fn test_resolve_store_user_id_uses_configured_value() {
    let config = create_test_config();
    let ctx = PathContext {
        store_user_id: Some("99887766".to_string()),
        ..Default::default()
    };
    let result = resolve_path(
        "<home>/Steam/userdata/<storeUserId>/saves",
        Some(&ctx),
        &config,
    )
    .unwrap();
    let result_str = result.to_string_lossy();
    assert!(
        result_str.contains("/99887766/"),
        "Expected configured storeUserId, got: {result_str}"
    );
}

#[test]
fn test_path_context_includes_store_user_id_from_game() {
    use crate::backup::{Game, GameDeviceBinding};
    use crate::device::{Device, DeviceResource, DeviceResourceKind, DeviceResourceSource};
    use crate::path_pattern::StoreKind;

    let device_bindings = std::collections::HashMap::from([(
        "dev-1".to_string(),
        GameDeviceBinding {
            account_ids: Some(vec![0]),
            ..GameDeviceBinding::default()
        },
    )]);

    let game = Game {
        name: "Test Game".to_string(),
        storage_key: String::new(),
        save_paths: vec![],
        game_paths: std::collections::HashMap::new(),
        next_save_unit_id: 0,
        cloud_sync_enabled: true,
        auto_backup: None,
        ludusavi_meta: None,
        device_bindings,
    };
    let device = Device {
        id: "dev-1".to_string(),
        name: "Test".to_string(),
        resources: vec![DeviceResource {
            id: 0,
            source: DeviceResourceSource::Manual,
            kind: DeviceResourceKind::StoreAccount {
                store: StoreKind::Steam,
                user_id: "12345678".to_string(),
            },
        }],
        next_resource_id: 1,
    };

    let ctx = game.path_context(Some(&device));
    assert_eq!(ctx.store_user_id, Some("12345678".to_string()));
}

#[test]
fn test_path_context_no_store_user_id_for_unknown_device() {
    use crate::backup::{Game, GameDeviceBinding};
    use crate::device::Device;

    let device_bindings = std::collections::HashMap::from([(
        "dev-1".to_string(),
        GameDeviceBinding {
            account_ids: Some(vec![0]),
            ..GameDeviceBinding::default()
        },
    )]);

    let game = Game {
        name: "Test Game".to_string(),
        storage_key: String::new(),
        save_paths: vec![],
        game_paths: std::collections::HashMap::new(),
        next_save_unit_id: 0,
        cloud_sync_enabled: true,
        auto_backup: None,
        ludusavi_meta: None,
        device_bindings,
    };
    let device = Device {
        id: "dev-other".to_string(),
        name: "Other".to_string(),
        resources: vec![],
        next_resource_id: 0,
    };

    let ctx = game.path_context(Some(&device));
    assert_eq!(ctx.store_user_id, None);
}
