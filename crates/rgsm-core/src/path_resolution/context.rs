use super::{GameInstallationCandidate, ResolutionContext, ResolutionSelection};
use crate::backup::Game;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// A pure projection of the device's discovered resources onto one game.
pub(crate) fn game_context(
    game: &Game,
    device: Option<&crate::device::Device>,
    environment: &ResolutionContext,
) -> ResolutionContext {
    let binding = device.and_then(|device| game.device_bindings.get(&device.id));
    let mut context = ResolutionContext {
        platform: environment.platform,
        platform_paths: environment.platform_paths.clone(),
        roots: environment.roots.clone(),
        accounts: environment.accounts.clone(),
        ..Default::default()
    };
    context.variables = crate::path_variables::effective(
        &device.map(|d| d.path_variables.clone()).unwrap_or_default(),
        binding.map(|b| &b.path_variables),
    );
    context.store_game_ids = game
        .ludusavi_meta
        .iter()
        .flat_map(|m| &m.store_game_ids)
        .map(|entry| (entry.store, entry.id.clone()))
        .collect::<BTreeMap<_, _>>();
    context.selection = ResolutionSelection {
        root_ids: binding.and_then(|b| selected_ids(b.root_ids.as_deref())),
        account_ids: binding.and_then(|b| selected_ids(b.account_ids.as_deref())),
        installation_ids: binding.and_then(|b| selected_ids(b.installation_ids.as_deref())),
    };
    context.installations = environment
        .installations
        .iter()
        .filter(|installation| {
            context
                .selection
                .installation_ids
                .as_ref()
                .is_some_and(|ids| ids.contains(&installation.id))
                || game.ludusavi_meta.as_ref().is_some_and(|meta| {
                    meta.install_dirs
                        .iter()
                        .any(|name| name.eq_ignore_ascii_case(&installation.install_dir))
                        || installation
                            .store_game_id
                            .as_deref()
                            .is_some_and(|id| meta.store_game_id(installation.store) == Some(id))
                })
        })
        .cloned()
        .collect();
    if let Some(path) = binding
        .and_then(|b| b.installation_path.as_deref())
        .filter(|p| !p.trim().is_empty())
    {
        let install_path = PathBuf::from(path);
        let known = environment
            .installations
            .iter()
            .find(|i| same_path(&i.install_path, &install_path));
        context.installations = vec![known.cloned().unwrap_or_else(|| {
            GameInstallationCandidate {
                id: "game-installation".into(),
                root_id: None,
                store: if context.store_game_ids.len() == 1 {
                    *context.store_game_ids.keys().next().unwrap()
                } else {
                    crate::path_pattern::StoreKind::Other
                },
                install_dir: install_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                install_path,
                store_game_id: None,
            }
        })];
        context.selection.installation_ids =
            Some([context.installations[0].id.clone()].into_iter().collect());
    }
    context
}

pub(crate) fn same_path(left: &std::path::Path, right: &std::path::Path) -> bool {
    let normalize = |path: &std::path::Path| {
        let value = path
            .to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_string();
        if cfg!(windows) {
            value.to_lowercase()
        } else {
            value
        }
    };
    normalize(left) == normalize(right)
}

fn selected_ids(ids: Option<&[String]>) -> Option<BTreeSet<String>> {
    ids.map(|ids| ids.iter().cloned().collect())
}
