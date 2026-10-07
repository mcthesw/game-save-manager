use super::*;
use crate::path_pattern::StoreKind;
use crate::path_resolution::{
    GameInstallationCandidate, GameRootCandidate, PlatformPaths, ResolutionContext,
    StoreAccountCandidate,
};

#[test]
fn literals_need_no_context_but_variables_do() {
    assert_eq!(
        resolve_path_explicit("/save/[literal].sav", None).unwrap(),
        PathBuf::from("/save/[literal].sav")
    );
    for expression in [
        "<base>/save",
        "<root>/save",
        "<storeGameId>/save",
        "<home>/save",
    ] {
        assert!(matches!(
            resolve_path_explicit(expression, None),
            Err(ResolveError::MissingContext(_))
        ));
    }
}

#[test]
fn exact_paths_use_the_same_context_and_parser_as_patterns() {
    let context = ResolutionContext {
        platform_paths: PlatformPaths {
            home: Some("/users/player".into()),
            os_user_name: Some("player".into()),
            ..Default::default()
        },
        installations: vec![GameInstallationCandidate {
            id: "installation".into(),
            root_id: None,
            store: StoreKind::Steam,
            install_dir: "Game[One]".into(),
            install_path: "D:/Games/Game[One]".into(),
            store_game_id: Some("10".into()),
        }],
        accounts: vec![StoreAccountCandidate {
            id: "account".into(),
            store: StoreKind::Steam,
            user_id: "99887766".into(),
        }],
        ..Default::default()
    };
    for (expression, expected) in [
        ("<home>/<osUserName>/save", "/users/player/player/save"),
        ("<base>/<game>/save", "D:/Games/Game[One]/Game[One]/save"),
        (
            "<base>/<storeGameId>/<storeUserId>/save",
            "D:/Games/Game[One]/10/99887766/save",
        ),
    ] {
        assert_eq!(
            resolve_path_explicit(expression, Some(&context)).unwrap(),
            PathBuf::from(expected)
        );
    }
    for expression in ["<unknown>/save", "<incomplete/save"] {
        assert!(resolve_path_explicit(expression, Some(&context)).is_err());
    }
}

#[test]
fn standalone_resolution_requires_a_root_choice() {
    let context = ResolutionContext {
        roots: ["/first", "/second"]
            .into_iter()
            .map(|path| GameRootCandidate {
                id: path.into(),
                path: path.into(),
                store: StoreKind::Steam,
            })
            .collect(),
        ..Default::default()
    };
    assert!(resolve_path_explicit("<root>/save", Some(&context)).is_err());
    let mut selected = context.clone();
    selected.selection.root_ids = Some(["/second".into()].into_iter().collect());
    assert_eq!(
        resolve_path_explicit("<root>/save", Some(&selected)).unwrap(),
        PathBuf::from("/second/save")
    );
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
        auto_backup_limit: None,
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
        path_variables: Default::default(),
        next_resource_id: 1,
    };

    let ctx = crate::services::game_path_context(&game, Some(&device));
    assert!(
        ctx.roots
            .iter()
            .any(|r| r.path == std::path::Path::new("/custom/root"))
    );
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
        auto_backup_limit: None,
        ludusavi_meta: None,
        device_bindings: std::collections::HashMap::new(),
    };

    let ctx = crate::services::game_path_context(&game, None);
    assert!(ctx.selection.root_ids.is_none());
}

#[test]
fn game_paths_require_a_root_selection_and_keep_other_devices_independent() {
    use crate::backup::{Game, GameDeviceBinding};
    use crate::device::{Device, DeviceResourceKind, DeviceResourceSource};
    use crate::path_pattern::StoreKind;

    let mut game: Game = serde_json::from_value(serde_json::json!({
        "name": "Path selection", "save_paths": []
    }))
    .unwrap();
    let mut device = Device {
        id: "path-device-a".into(),
        name: "A".into(),
        resources: vec![],
        path_variables: Default::default(),
        next_resource_id: 0,
    };
    for path in ["F:/Games", "H:/Games"] {
        device.add_resource(
            DeviceResourceSource::Manual,
            DeviceResourceKind::GameRoot {
                store: StoreKind::Other,
                path: path.into(),
            },
        );
    }
    let raw = "<root>/Example/slot[1].sav";
    assert!(
        resolve_path_explicit(
            raw,
            Some(&crate::services::game_path_context(&game, Some(&device)))
        )
        .is_err()
    );

    game.device_bindings.insert(
        device.id.clone(),
        GameDeviceBinding {
            root_ids: Some(vec!["resource:1".into()]),
            ..Default::default()
        },
    );
    let resolved = resolve_path_explicit(
        raw,
        Some(&crate::services::game_path_context(&game, Some(&device))),
    )
    .unwrap();
    assert_eq!(
        resolved,
        std::path::PathBuf::from("H:/Games/Example/slot[1].sav")
    );

    device.id = "path-device-b".into();
    assert!(
        resolve_path_explicit(
            raw,
            Some(&crate::services::game_path_context(&game, Some(&device)))
        )
        .is_err()
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
            account_ids: Some(vec!["resource:0".into()]),
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
        auto_backup_limit: None,
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
        path_variables: Default::default(),
        next_resource_id: 1,
    };

    let ctx = crate::services::game_path_context(&game, Some(&device));
    assert_eq!(
        resolve_path_explicit("<storeUserId>/save", Some(&ctx)).unwrap(),
        PathBuf::from("12345678/save")
    );
}

#[test]
fn test_path_context_no_store_user_id_for_unknown_device() {
    use crate::backup::{Game, GameDeviceBinding};
    use crate::device::Device;

    let device_bindings = std::collections::HashMap::from([(
        "dev-1".to_string(),
        GameDeviceBinding {
            account_ids: Some(vec!["resource:0".into()]),
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
        auto_backup_limit: None,
        ludusavi_meta: None,
        device_bindings,
    };
    let device = Device {
        id: "dev-other".to_string(),
        name: "Other".to_string(),
        resources: vec![],
        path_variables: Default::default(),
        next_resource_id: 0,
    };

    let ctx = crate::services::game_path_context(&game, Some(&device));
    assert!(ctx.selection.account_ids.is_none());
}

#[test]
fn manual_game_installation_is_local_to_its_game_and_needs_no_library() {
    let device: crate::device::Device = serde_json::from_value(serde_json::json!({
        "id":"local", "name":"Local", "resources":[]
    }))
    .unwrap();
    let game: crate::backup::Game = serde_json::from_value(serde_json::json!({
        "name":"Manual", "save_paths":[],
        "device_bindings":{"local":{"installationPath":"D:/Standalone/Game"}}
    }))
    .unwrap();
    assert_eq!(
        resolve_path_explicit(
            "<base>/Saves",
            Some(&crate::services::game_path_context(&game, Some(&device)))
        )
        .unwrap(),
        PathBuf::from("D:/Standalone/Game/Saves")
    );
}

#[test]
fn manual_game_does_not_inherit_another_games_installation() {
    let device: crate::device::Device = serde_json::from_value(serde_json::json!({
        "id":"local", "name":"Local", "resources":[
            {"id":0,"source":"manual","kind":{"type":"gameRoot","store":"other","path":"D:/Games"}},
            {"id":1,"source":"manual","kind":{"type":"gameInstallation","root_id":0,"store":"other","install_dir":"Other","path":"D:/Games/Other"}}
        ]
    })).unwrap();
    let game: crate::backup::Game =
        serde_json::from_value(serde_json::json!({"name":"Manual","save_paths":[]})).unwrap();
    assert!(
        resolve_path_explicit(
            "<base>/Saves",
            Some(&crate::services::game_path_context(&game, Some(&device)))
        )
        .is_err()
    );
}
