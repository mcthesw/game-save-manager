//! Read old global installation records only at the local configuration boundary.
//! Resolved installations live exclusively in the Game's Device binding. Records
//! that still need player input remain losslessly available for that input.
use std::{collections::BTreeSet, path::Path};

use crate::{
    backup::Game,
    device::{Device, DeviceResource, DeviceResourceKind},
    path_resolution::context::same_path,
};

use super::Config;

pub(crate) fn legacy_choices<'a>(game: &Game, device: &'a Device) -> Vec<&'a DeviceResource> {
    let binding = game.device_bindings.get(&device.id);
    device
        .game_installations()
        .filter(|resource| {
            if let Some(ids) = binding.and_then(|binding| binding.installation_ids.as_ref()) {
                return ids.contains(&format!("resource:{}", resource.id));
            }
            let DeviceResourceKind::GameInstallation {
                store,
                store_game_id,
                install_dir,
                root_id,
                ..
            } = &resource.kind
            else {
                return false;
            };
            if binding
                .and_then(|binding| binding.root_ids.as_ref())
                .is_some_and(|ids| !ids.contains(&format!("resource:{root_id}")))
            {
                return false;
            }
            game.ludusavi_meta.as_ref().is_some_and(|meta| {
                meta.matches_installation(*store, store_game_id.as_deref(), install_dir)
            })
        })
        .collect()
}

/// No filesystem discovery: another Device's paths and IDs are its own data.
pub(crate) fn migrate(config: &mut Config) -> bool {
    let mut changed = false;
    for (device_id, device) in &mut config.devices {
        let mut consumed = BTreeSet::new();
        for game in &mut config.games {
            let candidates = legacy_choices(game, device);
            let existing = game.device_bindings.get(device_id);
            if let Some(path) = existing
                .and_then(|binding| binding.installation_path.as_deref())
                .filter(|path| !path.trim().is_empty())
            {
                consumed.extend(candidates.iter().map(|resource| resource.id));
                consumed.extend(device.game_installations().filter(|resource| {
                    matches!(&resource.kind, DeviceResourceKind::GameInstallation { path: old, .. } if same_path(Path::new(old), Path::new(path)))
                }).map(|resource| resource.id));
                if let Some(binding) = game.device_bindings.get_mut(device_id) {
                    changed |= binding.installation_ids.take().is_some();
                }
                continue;
            }
            if candidates.is_empty() {
                continue;
            }
            let paths = candidates
                .iter()
                .filter_map(|resource| match &resource.kind {
                    DeviceResourceKind::GameInstallation { path, .. }
                        if !path.trim().is_empty() =>
                    {
                        Some(path.clone())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let ids = candidates
                .iter()
                .map(|resource| format!("resource:{}", resource.id))
                .collect::<Vec<_>>();
            let all_selected_found = existing
                .and_then(|binding| binding.installation_ids.as_ref())
                .is_none_or(|selected| selected.iter().all(|id| ids.contains(id)));
            let one_path = paths.first().filter(|first| {
                paths.len() == candidates.len()
                    && paths
                        .iter()
                        .all(|path| same_path(Path::new(first), Path::new(path)))
            });
            let binding = game.device_bindings.entry(device_id.clone()).or_default();
            if let Some(path) = one_path.filter(|_| all_selected_found) {
                binding.installation_path = Some(path.clone());
                binding.installation_ids = None;
                consumed.extend(candidates.iter().map(|resource| resource.id));
                changed = true;
            } else if binding.installation_ids.is_none() {
                // Preserve unresolved choices to block accidental automatic
                // selection. The resolver never expands the old global records.
                binding.installation_ids = Some(ids);
                changed = true;
            }
        }
        let pending = config
            .games
            .iter()
            .filter_map(|game| game.device_bindings.get(device_id))
            .filter_map(|binding| binding.installation_ids.as_ref())
            .flatten()
            .collect::<BTreeSet<_>>();
        let before = device.resources.len();
        device.resources.retain(|resource| {
            !consumed.contains(&resource.id)
                || pending.contains(&format!("resource:{}", resource.id))
        });
        changed |= before != device.resources.len();
    }
    changed
}

impl Config {
    pub(crate) fn normalize_local_fields(&mut self) {
        for game in &mut self.games {
            game.normalize_save_unit_ids();
        }
        migrate(self);
    }
}

#[cfg(test)]
#[path = "installation_upgrade_tests.rs"]
mod tests;
