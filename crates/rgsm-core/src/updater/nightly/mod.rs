//! Configuration shapes written only by pre-release nightly builds: per-device
//! `game_roots`, per-game `store_user_ids`, `ludusavi_meta.steamId` and global
//! installation records. Releases never wrote them, so none of this runs for a
//! configuration that comes from a released version.
use serde_json::Value;

use crate::backup::{GameDeviceBinding, StoreGameId};
use crate::config::Config;
use crate::device::{DeviceResourceKind, DeviceResourceSource};
use crate::path_pattern::StoreKind;

pub(crate) mod installations;
#[cfg(test)]
mod tests;

pub(super) fn detect(raw: &Value) -> bool {
    let device_roots = raw
        .get("devices")
        .and_then(Value::as_object)
        .is_some_and(|devices| {
            devices.values().any(|device| {
                device
                    .as_object()
                    .is_some_and(|device| device.contains_key("game_roots"))
            })
        });
    let game_fields = raw
        .get("games")
        .and_then(Value::as_array)
        .is_some_and(|games| {
            games.iter().any(|game| {
                game.as_object().is_some_and(|game| {
                    game.contains_key("store_user_ids")
                        || game
                            .get("ludusavi_meta")
                            .and_then(Value::as_object)
                            .is_some_and(|meta| {
                                meta.contains_key("steamId") || meta.contains_key("steam_id")
                            })
                })
            })
        });
    device_roots || game_fields
}

/// Moves the nightly fields into Device resources and Game bindings.
/// `detected_steam_roots` is only consulted for the current Device.
pub(super) fn apply(
    raw: &Value,
    config: &mut Config,
    current_device_id: Option<&str>,
    detected_steam_roots: &[String],
) {
    let detected_steam_roots = detected_steam_roots
        .iter()
        .map(|path| normalized_path_identity(path))
        .collect::<Vec<_>>();

    if let Some(raw_devices) = raw.get("devices").and_then(Value::as_object) {
        for (device_id, raw_device) in raw_devices {
            let Some(game_roots) = raw_device.get("game_roots").and_then(Value::as_array) else {
                continue;
            };
            let Some(device) = config.devices.get_mut(device_id) else {
                continue;
            };
            for path in game_roots.iter().filter_map(Value::as_str) {
                if device
                    .game_root_paths()
                    .any(|existing| existing.eq_ignore_ascii_case(path))
                {
                    continue;
                }
                let store = if Some(device_id.as_str()) == current_device_id
                    && detected_steam_roots.contains(&normalized_path_identity(path))
                {
                    StoreKind::Steam
                } else {
                    StoreKind::Other
                };
                device.add_resource(
                    DeviceResourceSource::Manual,
                    DeviceResourceKind::GameRoot {
                        store,
                        path: path.to_string(),
                    },
                );
            }
        }
    }

    let Some(raw_games) = raw.get("games").and_then(Value::as_array) else {
        return;
    };
    let (games, devices) = (&mut config.games, &mut config.devices);
    for (game, raw_game) in games.iter_mut().zip(raw_games.iter()) {
        if let Some(raw_units) = raw_game.get("save_paths").and_then(Value::as_array) {
            let root_devices = raw_units
                .iter()
                .filter_map(|unit| unit.get("paths").and_then(Value::as_object))
                .flat_map(|paths| paths.iter())
                .filter(|(_, path)| {
                    path.as_str()
                        .is_some_and(|path| path.to_ascii_lowercase().contains("<root>"))
                })
                .map(|(device_id, _)| device_id)
                .collect::<std::collections::BTreeSet<_>>();
            for device_id in root_devices {
                let unique_legacy_root = raw
                    .get("devices")
                    .and_then(Value::as_object)
                    .and_then(|devices| devices.get(device_id))
                    .and_then(|device| device.get("game_roots"))
                    .and_then(Value::as_array)
                    .and_then(|roots| match roots.as_slice() {
                        [root] => root.as_str(),
                        _ => None,
                    });
                let Some(root_id) = unique_legacy_root.and_then(|legacy_root| {
                    devices.get(device_id).and_then(|device| {
                        device
                            .resources
                            .iter()
                            .find_map(|resource| match &resource.kind {
                                DeviceResourceKind::GameRoot { path, .. }
                                    if normalized_path_identity(path)
                                        == normalized_path_identity(legacy_root) =>
                                {
                                    Some(resource.id)
                                }
                                _ => None,
                            })
                    })
                }) else {
                    continue;
                };
                let binding = game
                    .device_bindings
                    .entry(device_id.clone())
                    .or_insert_with(GameDeviceBinding::default);
                binding
                    .root_ids
                    .get_or_insert_with(|| vec![format!("resource:{root_id}")]);
            }
        }

        if let Some(steam_id) = raw_game
            .get("ludusavi_meta")
            .and_then(|meta| meta.get("steamId").or_else(|| meta.get("steam_id")))
            .and_then(Value::as_u64)
            && let Some(meta) = game.ludusavi_meta.as_mut()
            && meta.store_game_id(StoreKind::Steam).is_none()
        {
            meta.store_game_ids.push(StoreGameId {
                store: StoreKind::Steam,
                id: steam_id.to_string(),
            });
        }

        let Some(store_user_ids) = raw_game.get("store_user_ids").and_then(Value::as_object) else {
            continue;
        };
        for (device_id, user_id) in store_user_ids {
            let Some(user_id) = user_id.as_str() else {
                continue;
            };
            let Some(device) = devices.get_mut(device_id) else {
                continue;
            };
            let existing_account_id = device.store_accounts().find_map(|resource| match &resource
                .kind
            {
                DeviceResourceKind::StoreAccount {
                    store: StoreKind::Steam,
                    user_id: existing,
                } if existing == user_id => Some(resource.id),
                _ => None,
            });
            let account_id = match existing_account_id {
                Some(id) => id,
                None => device.add_resource(
                    DeviceResourceSource::Manual,
                    DeviceResourceKind::StoreAccount {
                        store: StoreKind::Steam,
                        user_id: user_id.to_string(),
                    },
                ),
            };
            game.device_bindings
                .entry(device_id.clone())
                .or_insert_with(GameDeviceBinding::default)
                .account_ids = Some(vec![format!("resource:{account_id}")]);
        }
    }
}

fn normalized_path_identity(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}
