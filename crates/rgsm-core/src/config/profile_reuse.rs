use std::collections::HashMap;

use super::{DeviceProfile, InitialCatchUpPolicy, SharedGame, SyncMode};
use crate::device::{DeviceResourceKind, DeviceResourceSource};

/// Copy locations, not identity, automation, archive ownership, or progress.
/// The caller supplies only accepted shared games, excluding local-only games.
pub(crate) fn reuse_profile_locations(
    current: &DeviceProfile,
    source: &DeviceProfile,
    games: &[SharedGame],
) -> (DeviceProfile, usize) {
    let mut accepted = current.clone();
    let mut resource_ids = HashMap::new();
    let mut count = 0;
    for game in games {
        let Some(old) = source
            .games
            .get(&game.storage_key)
            .filter(|game| game.visible)
        else {
            continue;
        };
        let Some(target) = accepted.games.get_mut(&game.storage_key) else {
            continue;
        };
        target.game_path = old.game_path.clone();
        for (id, settings) in &old.save_units {
            if game.save_units.iter().any(|unit| unit.id == *id) {
                target.save_units.insert(*id, settings.clone());
            }
        }
        target.binding = old.binding.clone();
        if let Some(binding) = &mut target.binding {
            for ids in [
                &mut binding.root_ids,
                &mut binding.account_ids,
                &mut binding.installation_ids,
            ]
            .into_iter()
            .flatten()
            {
                *ids = ids
                    .iter()
                    .filter_map(|id| {
                        remap_resource(*id, source, &mut accepted.device, &mut resource_ids)
                    })
                    .collect();
            }
            // Candidate IDs refer to the old device's resource dimensions.
            // Re-select a restore mapping on this device if it is ambiguous.
            binding.restore_mappings.clear();
        }
        target.visible = true;
        target.sync_mode = SyncMode::Manual;
        target.snapshot_sync_activation_revision = None;
        target.snapshot_sync_local_baseline.clear();
        target.initial_catch_up = InitialCatchUpPolicy::KeepRemote;
        target.auto_backup = None;
        target.live_save_process_name = None;
        target.live_save_snapshot_on_exit = false;
        count += 1;
    }
    (accepted, count)
}

fn remap_resource(
    id: u32,
    source: &DeviceProfile,
    current: &mut crate::device::Device,
    remapped: &mut HashMap<u32, u32>,
) -> Option<u32> {
    if let Some(mapped) = remapped.get(&id) {
        return Some(*mapped);
    }
    let mut kind = source.device.resource(id)?.kind.clone();
    if let DeviceResourceKind::GameInstallation { root_id, .. } = &mut kind {
        // Reject malformed chains instead of recursing through installation cycles.
        if !matches!(
            source.device.resource(*root_id)?.kind,
            DeviceResourceKind::GameRoot { .. }
        ) {
            return None;
        }
        *root_id = remap_resource(*root_id, source, current, remapped)?;
    }
    let mapped = current
        .resources
        .iter()
        .find(|resource| resource.kind == kind)
        .map(|r| r.id)
        .unwrap_or_else(|| current.add_resource(DeviceResourceSource::Manual, kind));
    remapped.insert(id, mapped);
    Some(mapped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ConfigurationOwners};
    use crate::path_pattern::StoreKind;

    #[test]
    fn reuses_locations_without_identity_progress_automation_or_private_games() {
        let config = Config {
            games: serde_json::from_value(serde_json::json!([
                {"name":"Shared", "storage_key":"shared", "save_paths":[{"id":1,"unit_type":"File","paths":{"old":"D:/old.sav","new":"E:/new.sav"}}]},
                {"name":"Private", "storage_key":"private", "save_paths":[]}
            ])).unwrap(),
            ..Config::default()
        };
        let owners = ConfigurationOwners::from_legacy(&config, &"new".into());
        let mut current = owners.device_profiles["new"].clone();
        let mut old = owners.device_profiles["old"].clone();
        let kind = |path: &str| DeviceResourceKind::GameRoot {
            store: StoreKind::Steam,
            path: path.into(),
        };
        let existing = current
            .device
            .add_resource(DeviceResourceSource::Manual, kind("E:/Steam"));
        let source_id = old
            .device
            .add_resource(DeviceResourceSource::Detected, kind("D:/Steam"));
        assert_eq!(existing, source_id);
        old.games.get_mut("shared").unwrap().binding = Some(crate::backup::GameDeviceBinding {
            root_ids: Some(vec![source_id]),
            ..Default::default()
        });
        old.games.get_mut("shared").unwrap().sync_mode = SyncMode::MultiDeviceSync;
        old.games
            .get_mut("shared")
            .unwrap()
            .snapshot_sync_activation_revision = Some(40);
        old.games
            .get_mut("shared")
            .unwrap()
            .live_save_snapshot_on_exit = true;
        old.games.get_mut("shared").unwrap().auto_backup = Some(crate::backup::AutoBackupConfig {
            interval_secs: 60,
            max_backup_count: None,
        });
        let games = owners
            .shared_library
            .games
            .iter()
            .filter(|game| game.storage_key == "shared")
            .cloned()
            .collect::<Vec<_>>();
        let (accepted, count) = reuse_profile_locations(&current, &old, &games);
        assert_eq!(count, 1);
        assert_eq!(accepted.device.id, current.device.id);
        assert_eq!(accepted.device.name, current.device.name);
        assert_eq!(accepted.local_archive_root, current.local_archive_root);
        assert_eq!(accepted.quick_action, current.quick_action);
        assert_eq!(accepted.behavior, current.behavior);
        assert_eq!(accepted.private_favorites, current.private_favorites);
        assert_eq!(accepted.games["private"], current.games["private"]);
        let reused = &accepted.games["shared"];
        assert_eq!(reused.save_units[&1].path.as_deref(), Some("D:/old.sav"));
        assert_eq!(reused.sync_mode, SyncMode::Manual);
        assert_eq!(reused.auto_backup, None);
        assert_eq!(reused.snapshot_sync_activation_revision, None);
        assert!(!reused.live_save_snapshot_on_exit);
        assert!(reused.snapshot_sync_local_baseline.is_empty());
        let root_id = reused.binding.as_ref().unwrap().root_ids.as_ref().unwrap()[0];
        assert_ne!(root_id, existing);
        assert_eq!(
            accepted.device.resource(root_id).unwrap().kind,
            kind("D:/Steam")
        );
        let (again, _) = reuse_profile_locations(&accepted, &old, &games);
        assert_eq!(accepted, again);
    }
}
