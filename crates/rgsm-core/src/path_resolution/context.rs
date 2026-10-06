use super::{
    GameInstallationCandidate, GameRootCandidate, PlatformPaths, ResolutionContext,
    ResolutionSelection, StoreAccountCandidate,
};
use crate::backup::Game;
use crate::device::DeviceResourceKind;
use crate::path_pattern::PlatformKind;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub(crate) fn game_context(
    game: &Game,
    device: Option<&crate::device::Device>,
) -> ResolutionContext {
    let mut roots = Vec::new();
    let mut accounts = Vec::new();
    let mut installations = Vec::new();
    if let Some(device) = device {
        for resource in &device.resources {
            let id = resource_id(resource.id);
            match &resource.kind {
                DeviceResourceKind::GameRoot { store, path } => roots.push(GameRootCandidate {
                    id,
                    store: *store,
                    path: PathBuf::from(path),
                }),
                DeviceResourceKind::StoreAccount { store, user_id } => {
                    accounts.push(StoreAccountCandidate {
                        id,
                        store: *store,
                        user_id: user_id.clone(),
                    });
                }
                DeviceResourceKind::GameInstallation {
                    root_id,
                    store,
                    install_dir,
                    path,
                    store_game_id,
                } => {
                    let applies = game.ludusavi_meta.as_ref().is_none_or(|meta| {
                        meta.install_dirs
                            .iter()
                            .any(|candidate| candidate.eq_ignore_ascii_case(install_dir))
                            || store_game_id.as_deref().is_some_and(|id| {
                                meta.store_game_id(*store)
                                    .is_some_and(|candidate| candidate == id)
                            })
                    });
                    if applies {
                        installations.push(GameInstallationCandidate {
                            id,
                            root_id: resource_id(*root_id),
                            store: *store,
                            install_dir: install_dir.clone(),
                            install_path: PathBuf::from(path),
                            store_game_id: store_game_id.clone(),
                        });
                    }
                }
            }
        }
    }
    add_detected_steam_resources(game, &mut roots, &mut accounts, &mut installations);

    let binding = device.and_then(|device| game.device_bindings.get(&device.id));
    ResolutionContext {
        variables: crate::path_variables::effective(
            &device.map(|d| d.path_variables.clone()).unwrap_or_default(),
            binding.map(|b| &b.path_variables),
        ),
        platform: PlatformKind::host(),
        platform_paths: host_platform_paths(),
        roots,
        accounts,
        installations,
        store_game_ids: game
            .ludusavi_meta
            .iter()
            .flat_map(|meta| &meta.store_game_ids)
            .map(|entry| (entry.store, entry.id.clone()))
            .collect::<BTreeMap<_, _>>(),
        selection: ResolutionSelection {
            root_ids: binding.and_then(|value| selected_ids(value.root_ids.as_deref())),
            account_ids: binding.and_then(|value| selected_ids(value.account_ids.as_deref())),
            installation_ids: binding
                .and_then(|value| selected_ids(value.installation_ids.as_deref())),
        },
    }
}

fn add_detected_steam_resources(
    game: &Game,
    roots: &mut Vec<GameRootCandidate>,
    accounts: &mut Vec<StoreAccountCandidate>,
    installations: &mut Vec<GameInstallationCandidate>,
) {
    let libraries = crate::steam::get_steam_library_paths().unwrap_or_default();
    for library in &libraries {
        if roots.iter().any(|root| same_path(&root.path, library)) {
            continue;
        }
        roots.push(GameRootCandidate {
            id: detected_id("steam-root", &library.to_string_lossy()),
            store: crate::path_pattern::StoreKind::Steam,
            path: library.clone(),
        });
    }
    for account in crate::steam::detect_steam_user_ids().unwrap_or_default() {
        if accounts.iter().any(|known| {
            known.store == crate::path_pattern::StoreKind::Steam && known.user_id == account.user_id
        }) {
            continue;
        }
        accounts.push(StoreAccountCandidate {
            id: detected_id("steam-account", &account.user_id),
            store: crate::path_pattern::StoreKind::Steam,
            user_id: account.user_id,
        });
    }

    let Some(meta) = &game.ludusavi_meta else {
        return;
    };
    let games = crate::steam::scan_all_installed_games().unwrap_or_default();
    for install_dir in &meta.install_dirs {
        let Some(installed) = games.get(&install_dir.to_lowercase()) else {
            continue;
        };
        let Some(root) = roots.iter().find(|root| {
            root.store == crate::path_pattern::StoreKind::Steam
                && installed
                    .install_path
                    .starts_with(root.path.join("steamapps").join("common"))
        }) else {
            continue;
        };
        if installations
            .iter()
            .any(|known| same_path(&known.install_path, &installed.install_path))
        {
            continue;
        }
        installations.push(GameInstallationCandidate {
            id: detected_id("steam-install", &installed.install_path.to_string_lossy()),
            root_id: root.id.clone(),
            store: crate::path_pattern::StoreKind::Steam,
            install_dir: installed.install_dir.clone(),
            install_path: installed.install_path.clone(),
            store_game_id: Some(installed.app_id.to_string()),
        });
    }
}

pub(crate) fn detected_id(kind: &str, value: &str) -> String {
    format!(
        "detected:{kind}:{}",
        value.replace('\\', "/").to_lowercase()
    )
}

fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .replace('\\', "/")
        .eq_ignore_ascii_case(&right.to_string_lossy().replace('\\', "/"))
}

fn selected_ids(ids: Option<&[u32]>) -> Option<BTreeSet<String>> {
    ids.map(|ids| ids.iter().copied().map(resource_id).collect())
}

fn resource_id(id: u32) -> String {
    format!("resource:{id}")
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
