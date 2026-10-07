use std::path::PathBuf;

use crate::{
    backup::Game,
    device::{Device, DeviceResourceKind},
    path_pattern::StoreKind,
    path_resolution::context::same_path,
    path_resolution::{
        GameInstallationCandidate, GameRootCandidate, PlatformPaths, ResolutionContext,
        StoreAccountCandidate,
    },
};

/// Scan once per operation; callers reuse this snapshot for every Save Unit.
pub(crate) fn device_path_environment(device: Option<&Device>) -> ResolutionContext {
    device_path_environment_with_discovery(device, true)
}

fn device_path_environment_with_discovery(
    device: Option<&Device>,
    discover: bool,
) -> ResolutionContext {
    let mut environment = ResolutionContext {
        platform_paths: host_platform_paths(),
        ..Default::default()
    };
    for resource in device.into_iter().flat_map(|d| &d.resources) {
        let id = format!("resource:{}", resource.id);
        match &resource.kind {
            DeviceResourceKind::GameRoot { store, path } => {
                environment.roots.push(GameRootCandidate {
                    id,
                    store: *store,
                    path: path.into(),
                })
            }
            DeviceResourceKind::StoreAccount { store, user_id } => {
                environment.accounts.push(StoreAccountCandidate {
                    id,
                    store: *store,
                    user_id: user_id.clone(),
                })
            }
            DeviceResourceKind::GameInstallation { .. } => {}
        }
    }
    if !discover {
        return environment;
    }
    for library in crate::steam::get_steam_library_paths().unwrap_or_default() {
        if !environment
            .roots
            .iter()
            .any(|r| same_path(&r.path, &library))
        {
            environment.roots.push(GameRootCandidate {
                id: detected_id("steam-root", &library.to_string_lossy()),
                store: StoreKind::Steam,
                path: library,
            });
        }
    }
    for account in crate::steam::detect_steam_user_ids().unwrap_or_default() {
        if !environment
            .accounts
            .iter()
            .any(|a| a.store == StoreKind::Steam && a.user_id == account.user_id)
        {
            environment.accounts.push(StoreAccountCandidate {
                id: detected_id("steam-account", &account.user_id),
                store: StoreKind::Steam,
                user_id: account.user_id,
            });
        }
    }
    for root in environment
        .roots
        .iter()
        .filter(|r| r.store == StoreKind::Steam)
    {
        for installed in crate::steam::scan_library_manifests(&root.path) {
            if environment
                .installations
                .iter()
                .any(|i| same_path(&i.install_path, &installed.install_path))
            {
                continue;
            }
            environment.installations.push(GameInstallationCandidate {
                id: detected_id("steam-install", &installed.install_path.to_string_lossy()),
                root_id: Some(root.id.clone()),
                store: StoreKind::Steam,
                install_dir: installed.install_dir,
                install_path: installed.install_path,
                store_game_id: Some(installed.app_id.to_string()),
            });
        }
    }
    environment
}

pub fn device_path_context(device: Option<&Device>) -> crate::path_resolver::PathContext {
    let mut context = device_path_environment(device);
    context.installations.clear();
    context.variables = device.map(|d| d.path_variables.clone()).unwrap_or_default();
    context
}

pub fn game_path_context(
    game: &Game,
    device: Option<&Device>,
) -> crate::path_resolver::PathContext {
    game.path_context(device, &device_path_environment(device))
}

pub(crate) fn detected_id(kind: &str, value: &str) -> String {
    let value = value.replace('\\', "/");
    let value = if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    };
    format!("detected:{kind}:{value}")
}

fn host_platform_paths() -> PlatformPaths {
    let home = dirs::home_dir();
    PlatformPaths {
        home: home.clone(),
        os_user_name: Some(whoami::username()),
        win_app_data: dirs::data_dir(),
        win_local_app_data: dirs::data_local_dir(),
        win_local_app_data_low: home.map(|path| path.join("AppData").join("LocalLow")),
        win_documents: dirs::document_dir(),
        win_public: std::env::var_os("PUBLIC").map(PathBuf::from),
        win_program_data: std::env::var_os("PROGRAMDATA").map(PathBuf::from),
        win_dir: std::env::var_os("WINDIR").map(PathBuf::from),
        xdg_data: if cfg!(target_os = "windows") {
            None
        } else {
            dirs::data_dir()
        },
        xdg_config: if cfg!(target_os = "windows") {
            None
        } else {
            dirs::config_dir()
        },
    }
}

#[derive(Debug, serde::Serialize, specta::Type, utoipa::ToSchema)]
pub struct GameLocationOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, serde::Serialize, specta::Type, utoipa::ToSchema)]
pub struct GameLocationOptions {
    pub roots: Vec<GameLocationOption>,
    pub accounts: Vec<GameLocationOption>,
    pub installations: Vec<String>,
}

pub fn game_location_options(
    game: &Game,
    device: Option<&Device>,
    discover: bool,
) -> GameLocationOptions {
    let environment = device_path_environment_with_discovery(device, discover);
    let mut draft = game.clone();
    for binding in draft.device_bindings.values_mut() {
        binding.installation_path = None;
        binding.installation_ids = None;
    }
    let context = crate::path_resolution::context::game_context(&draft, device, &environment);
    let mut installations = context
        .installations
        .iter()
        .map(|i| i.install_path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    for resource in device
        .into_iter()
        .flat_map(|device| crate::config::installation_upgrade::legacy_choices(game, device))
    {
        if let DeviceResourceKind::GameInstallation { path, .. } = &resource.kind
            && !installations.iter().any(|existing| {
                same_path(
                    PathBuf::from(existing).as_path(),
                    PathBuf::from(path).as_path(),
                )
            })
        {
            installations.push(path.clone());
        }
    }
    GameLocationOptions {
        roots: context
            .roots
            .into_iter()
            .map(|r| GameLocationOption {
                id: r.id,
                label: format!("{:?} · {}", r.store, r.path.display()),
            })
            .collect(),
        accounts: context
            .accounts
            .into_iter()
            .map(|a| GameLocationOption {
                id: a.id,
                label: format!("{:?} · {}", a.store, a.user_id),
            })
            .collect(),
        installations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::DeviceResourceSource;

    #[test]
    fn unresolved_global_installations_are_choices_only_and_do_not_drive_runtime_paths() {
        let device: Device = serde_json::from_value(serde_json::json!({
            "id":"test", "name":"Test", "resources":[{"id":1,"source":"manual","kind":{
                "type":"gameInstallation", "root_id":0,"store":"steam","install_dir":"Game","path":"D:/Game", "store_game_id":"42"
            }}]
        })).unwrap();
        let game: Game = serde_json::from_value(serde_json::json!({"name":"Test", "save_paths":[], "device_bindings":{"test":{"installationIds":[1]}}})).unwrap();
        let environment = device_path_environment_with_discovery(Some(&device), false);
        assert!(environment.installations.is_empty());
        let context =
            crate::path_resolution::context::game_context(&game, Some(&device), &environment);
        let expression = crate::path_pattern::parse_manifest_path_pattern("<base>/Saves").unwrap();
        let plan =
            crate::path_resolution::plan_resolution(&expression, Default::default(), &context);
        assert!(plan.candidates.is_empty());
        assert_eq!(
            game_location_options(&game, Some(&device), false).installations,
            vec!["D:/Game"]
        );
    }

    #[test]
    fn scanning_keeps_each_installation_and_context_survives_manifest_changes() {
        let temp = temp_dir::TempDir::new().unwrap();
        let mut device: Device =
            serde_json::from_value(serde_json::json!({"id":"test", "name":"Test"})).unwrap();
        let mut manifests = Vec::new();
        for name in ["Library A", "Library B"] {
            let root = temp.path().join(name);
            std::fs::create_dir_all(root.join("steamapps/common/Shared Name")).unwrap();
            let manifest = root.join("steamapps/appmanifest_987654321.acf");
            std::fs::write(&manifest, "\"AppState\" { \"appid\" \"987654321\" \"name\" \"Test\" \"installdir\" \"Shared Name\" }").unwrap();
            manifests.push(manifest);
            device.add_resource(
                DeviceResourceSource::Manual,
                DeviceResourceKind::GameRoot {
                    store: StoreKind::Steam,
                    path: root.to_string_lossy().into_owned(),
                },
            );
        }
        let environment = device_path_environment(Some(&device));
        for manifest in manifests {
            std::fs::remove_file(manifest).unwrap();
        }
        let game: Game = serde_json::from_value(serde_json::json!({"name":"Test", "save_paths":[], "ludusavi_meta":{"installDirs":[], "storeGameIds":[{"store":"steam","id":"987654321"}]}})).unwrap();
        let context =
            crate::path_resolution::context::game_context(&game, Some(&device), &environment);
        assert_eq!(context.installations.len(), 2);
        for pattern in ["<base>/Saves/*.sav", "<base>/Settings.ini"] {
            let parsed = crate::path_pattern::parse_manifest_path_pattern(pattern).unwrap();
            let plan =
                crate::path_resolution::plan_resolution(&parsed, Default::default(), &context);
            assert_eq!(plan.candidates.len(), 2);
            assert!(matches!(
                plan.selection_state,
                crate::path_resolution::ResolutionSelectionState::Ambiguous { .. }
            ));
        }
    }
}
