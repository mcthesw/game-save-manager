use std::fs;
use std::path::{Path, PathBuf};

use log::{error, info, warn};
use semver::Version;
use serde_json::Value;

use crate::backup::storage_key::generate_unique_storage_key;
use crate::backup::{GameSnapshots, SaveUnitSource, TIMER_AUTO_BACKUP_DESCRIPTION};
use crate::config::{Config, resolve_backup_path};
use crate::device::get_current_device_id;
use crate::path_pattern::{ManifestPathConstraints, ManifestPathPattern, is_dynamic_manifest_path};
use crate::preclude::UpdaterError;
use crate::updater::{
    nightly,
    probe::probe_config_version,
    versions::{
        CURRENT_VERSION, Config1_4_0, MIN_SUPPORTED_VERSION, VERSION_1_4_0, VERSION_1_6_0,
        VERSION_1_7_5, VERSION_1_8_1, VERSION_1_9_0,
    },
};

/// Update configuration file to the latest version
///
/// This function handles the entire migration process:
/// 1. Version probing
/// 2. Version compatibility check
/// 3. Backup creation
/// 4. Data migration
/// 5. New config writing
///
/// # Arguments
/// * `path` - Path to the config file
///
/// # Returns
/// * `Ok(true)` - If migration succeeds and writes an updated config
/// * `Ok(false)` - If no migration is needed
/// * `Err(UpdaterError)` - If any step fails
pub fn update_config<P: AsRef<Path>>(path: P) -> Result<bool, UpdaterError> {
    let path: &Path = path.as_ref();
    let content = fs::read_to_string(path)?;
    let raw: Value = serde_json::from_str(&content)?;
    let nightly = nightly::detect(&raw);
    let needs_path_migration = nightly || has_units_without_source(&raw);
    let version = probe_config_version(path)?;
    let current = Version::parse(CURRENT_VERSION)?;
    let min_supported = Version::parse(MIN_SUPPORTED_VERSION)?;
    let version_1_6_0 = Version::parse(VERSION_1_6_0)?;
    let version_1_8_1 = Version::parse(VERSION_1_8_1)?;
    let version_1_9_0 = Version::parse(VERSION_1_9_0)?;

    // Version compatibility check
    if version > current {
        error!(
            target: "rgsm::updater",
            "Config version too new in {}: {} > {}",
            path.display(),
            version,
            current
        );
        return Err(UpdaterError::ConfigVersionTooNew {
            path: path.to_path_buf(),
            found: version,
            current,
        });
    }
    if version < min_supported {
        error!(
            target: "rgsm::updater",
            "Config version too old in {}: {} < {}",
            path.display(),
            version,
            min_supported
        );
        return Err(UpdaterError::ConfigVersionTooOld {
            path: path.to_path_buf(),
            found: version,
            min_supported,
        });
    }
    if version == current && !needs_path_migration {
        return Ok(false);
    }

    warn!(target: "rgsm::updater", "Config schema requires migration, updating...");
    // Create backup
    backup_config(path)?;

    // Migrate based on version
    let detected_steam_roots = if nightly {
        crate::steam::detect_game_roots().unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut new_cfg = migrate_config_schema(
        &content,
        &version,
        Some(get_current_device_id()),
        &detected_steam_roots,
    )?;

    let backup_path = resolve_backup_path(&new_cfg.backup_path);

    // Migrate snapshot source metadata before any snapshot-file rewrites that
    // would otherwise serialize legacy timer snapshots as Manual.
    if version < version_1_8_1 {
        migrate_snapshot_created_by_metadata(&backup_path)?;
    }

    // Migrate game snapshots if upgrading from before 1.6.0
    if version < version_1_6_0 {
        migrate_game_snapshots_to_chain(&backup_path)?;
    }

    // Backfill storage_key for all games if upgrading from before 1.9.0
    if version < version_1_9_0 {
        new_cfg = migrate_storage_keys(new_cfg, &backup_path)?;
    }

    write_config_transactionally(path, serde_json::to_string_pretty(&new_cfg)?.as_bytes())?;
    info!(target: "rgsm::updater", "Config updated successfully to version {}", CURRENT_VERSION);
    Ok(true)
}

/// Upgrade configuration fields without reading or writing local/remote files.
/// Storage identities are assigned separately using each storage layout's rules.
pub(super) fn migrate_config_schema(
    content: &str,
    version: &Version,
    current_device_id: Option<&str>,
    detected_steam_roots: &[String],
) -> Result<Config, UpdaterError> {
    let mut config = migrate_legacy_cloud_sync(content, migrate_config(content, version)?)?;
    let raw: Value = serde_json::from_str(content)?;
    convert_units_without_source(&raw, &mut config);
    if nightly::detect(&raw) {
        nightly::apply(&raw, &mut config, current_device_id, detected_steam_roots);
    }
    if version < &Version::parse(VERSION_1_7_5)? {
        config = migrate_save_unit_ids(config);
    }
    Ok(config)
}

fn has_units_without_source(raw: &Value) -> bool {
    raw.get("games")
        .and_then(Value::as_array)
        .is_some_and(|games| {
            games.iter().any(|game| {
                game.get("save_paths")
                    .and_then(Value::as_array)
                    .is_some_and(|units| {
                        units.iter().any(|unit| {
                            unit.as_object()
                                .is_some_and(|unit| !unit.contains_key("source"))
                        })
                    })
            })
        })
}

/// Units written before `source` existed carry a plain per-device path map; a
/// single dynamic pattern shared by every device becomes a manifest pattern.
fn convert_units_without_source(raw: &Value, config: &mut Config) {
    let Some(raw_games) = raw.get("games").and_then(Value::as_array) else {
        return;
    };
    for (game, raw_game) in config.games.iter_mut().zip(raw_games.iter()) {
        let Some(raw_units) = raw_game.get("save_paths").and_then(Value::as_array) else {
            continue;
        };
        for (unit, raw_unit) in game.save_paths.iter_mut().zip(raw_units.iter()) {
            if raw_unit
                .as_object()
                .is_some_and(|unit| unit.contains_key("source"))
            {
                continue;
            }
            let SaveUnitSource::Concrete { unit_type, paths } = &unit.source else {
                continue;
            };
            let expected_type = unit_type.clone();
            let unique_paths = paths.values().collect::<std::collections::BTreeSet<_>>();
            let patterns = unique_paths.into_iter().collect::<Vec<_>>();
            if let [raw_pattern] = patterns.as_slice()
                && is_dynamic_manifest_path(raw_pattern)
            {
                unit.source = SaveUnitSource::ManifestPattern {
                    expected_type: Some(expected_type),
                    pattern: ManifestPathPattern::new((*raw_pattern).clone()),
                    constraints: ManifestPathConstraints::default(),
                };
            }
        }
    }
}

fn write_config_transactionally(path: &Path, content: &[u8]) -> Result<(), UpdaterError> {
    crate::atomic_file::write_bytes_atomically(path, content)?;
    Ok(())
}

/// Migrate config content based on its version
fn migrate_config(content: &str, version: &Version) -> Result<Config, UpdaterError> {
    if version.to_string().as_str() <= VERSION_1_4_0 {
        let old_cfg: Config1_4_0 = serde_json::from_str(content)?;
        Ok(Config::from(old_cfg))
    } else {
        // Try direct deserialization for compatible versions
        let mut new_cfg: Config = serde_json::from_str(content)?;
        new_cfg.version = CURRENT_VERSION.to_string();
        Ok(new_cfg)
    }
}

fn migrate_legacy_cloud_sync(content: &str, mut config: Config) -> Result<Config, UpdaterError> {
    let raw: Value = serde_json::from_str(content)?;
    let legacy_always_sync = raw
        .pointer("/settings/cloud_settings/always_sync")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if let Some(raw_games) = raw.get("games").and_then(Value::as_array) {
        for (game, raw_game) in config.games.iter_mut().zip(raw_games.iter()) {
            let has_cloud_sync_enabled = raw_game
                .as_object()
                .map(|obj| obj.contains_key("cloud_sync_enabled"))
                .unwrap_or(true);
            if !has_cloud_sync_enabled {
                game.cloud_sync_enabled = legacy_always_sync;
            }
        }
    }

    Ok(config)
}

fn migrate_snapshot_created_by_metadata(backup_path: &Path) -> Result<(), UpdaterError> {
    migrate_snapshot_created_by_in_backup_root(backup_path)
}

fn migrate_snapshot_created_by_in_backup_root(backup_path: &Path) -> Result<(), UpdaterError> {
    if !backup_path.exists() {
        info!(
            target: "rgsm::updater",
            "Backup path does not exist, skipping snapshot source migration"
        );
        return Ok(());
    }

    let entries = fs::read_dir(backup_path)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let backups_json_path = path.join("Backups.json");
        if !backups_json_path.exists() {
            continue;
        }

        let content = match fs::read_to_string(&backups_json_path) {
            Ok(content) => content,
            Err(err) => {
                warn!(
                    target: "rgsm::updater",
                    "Failed to read {:?}: {}",
                    backups_json_path,
                    err
                );
                continue;
            }
        };

        let mut raw: Value = match serde_json::from_str(&content) {
            Ok(raw) => raw,
            Err(err) => {
                warn!(
                    target: "rgsm::updater",
                    "Failed to parse {:?}: {}",
                    backups_json_path,
                    err
                );
                continue;
            }
        };

        let migrated_count = migrate_snapshot_created_by_fields(&mut raw);
        if migrated_count == 0 {
            continue;
        }

        super::preserve_original(&backups_json_path, content.as_bytes())?;
        crate::atomic_file::write_bytes_atomically(
            &backups_json_path,
            &serde_json::to_vec_pretty(&raw)?,
        )?;
        info!(
            target: "rgsm::updater",
            "Migrated {} snapshot source entries in {:?}",
            migrated_count,
            backups_json_path
        );
    }

    Ok(())
}

fn migrate_snapshot_created_by_fields(raw: &mut Value) -> usize {
    let Some(backups) = raw.get_mut("backups").and_then(Value::as_array_mut) else {
        return 0;
    };

    let mut migrated = 0;
    for snapshot in backups {
        let Some(snapshot) = snapshot.as_object_mut() else {
            continue;
        };

        if snapshot.contains_key("created_by") {
            continue;
        }

        let created_by = match snapshot.get("describe").and_then(Value::as_str) {
            Some(TIMER_AUTO_BACKUP_DESCRIPTION) => "Timer",
            _ => "Manual",
        };
        snapshot.insert(
            "created_by".to_string(),
            Value::String(created_by.to_string()),
        );
        migrated += 1;
    }

    migrated
}

/// Create a backup of the config file
fn backup_config<P: AsRef<Path>>(path: P) -> Result<PathBuf, UpdaterError> {
    let path = path.as_ref();
    Ok(super::preserve_original(path, &fs::read(path)?)?)
}

/// Migrate game snapshots from flat list to chained structure (for versions < 1.6.0)
///
/// This function:
/// 1. Scans all game folders in the backup path
/// 2. For each Backups.json, sorts snapshots by date (ascending)
/// 3. Creates a parent chain from oldest to newest
/// 4. Sets head to the newest snapshot
fn migrate_game_snapshots_to_chain(backup_path: &Path) -> Result<(), UpdaterError> {
    if !backup_path.exists() {
        info!(target: "rgsm::updater", "Backup path does not exist, skipping snapshot migration");
        return Ok(());
    }

    // Iterate through all directories in backup path
    let entries = fs::read_dir(backup_path)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let backups_json_path = path.join("Backups.json");
        if !backups_json_path.exists() {
            continue;
        }

        // Read the existing Backups.json
        let content = match fs::read_to_string(&backups_json_path) {
            Ok(c) => c,
            Err(e) => {
                warn!(target: "rgsm::updater", "Failed to read {:?}: {}", backups_json_path, e);
                continue;
            }
        };

        let mut game_snapshots: GameSnapshots = match serde_json::from_str(&content) {
            Ok(gs) => gs,
            Err(e) => {
                warn!(target: "rgsm::updater", "Failed to parse {:?}: {}", backups_json_path, e);
                continue;
            }
        };

        game_snapshots.normalize_heads();

        // Skip if already migrated (has parent or device head set)
        if !game_snapshots.device_heads.is_empty()
            || game_snapshots.backups.iter().any(|s| s.parent.is_some())
        {
            info!(target: "rgsm::updater", "Skipping {:?}: already has chain structure", backups_json_path);
            continue;
        }

        // Skip if no snapshots or only one snapshot
        if game_snapshots.backups.len() <= 1 {
            // For single snapshot, just set it as head
            if let Some(snapshot) = game_snapshots.backups.first() {
                game_snapshots.set_current_device_head(Some(snapshot.date.clone()));
            }
        } else {
            // Sort snapshots by date (ascending - oldest first)
            game_snapshots
                .backups
                .sort_by_key(|snapshot| snapshot.creation_time());

            // Create parent chain: each snapshot points to the previous one
            for i in 1..game_snapshots.backups.len() {
                let parent_date = game_snapshots.backups[i - 1].date.clone();
                game_snapshots.backups[i].parent = Some(parent_date);
            }

            // Set head to the newest (last) snapshot
            if let Some(newest) = game_snapshots.backups.last() {
                game_snapshots.set_current_device_head(Some(newest.date.clone()));
            }
        }

        // Write updated Backups.json
        super::preserve_original(&backups_json_path, content.as_bytes())?;
        crate::atomic_file::write_bytes_atomically(
            &backups_json_path,
            &serde_json::to_vec_pretty(&game_snapshots)?,
        )?;
        info!(target: "rgsm::updater", "Migrated snapshots to chain structure: {:?}", backups_json_path);
    }

    info!(target: "rgsm::updater", "Game snapshots migration completed");
    Ok(())
}

/// Assign stable IDs to save units that were created before v1.7.5.
///
/// Old save units have `id: 0` (the serde default). This migration assigns
/// sequential IDs starting from 0 and sets `next_save_unit_id` to one past
/// the last assigned ID. This preserves the positional-index mapping that
/// old V2 archives were created with.
fn migrate_save_unit_ids(mut config: Config) -> Config {
    for game in &mut config.games {
        // Skip games that already have non-zero IDs (already migrated or created post-1.7.5)
        let needs_migration = game.next_save_unit_id == 0
            && game.save_paths.iter().all(|u| u.id == 0)
            && !game.save_paths.is_empty();

        if !needs_migration {
            continue;
        }

        // Assign IDs matching the positional index so existing V2 archives
        // (which used `enumerate()` index as prefix) remain compatible.
        for (i, unit) in game.save_paths.iter_mut().enumerate() {
            unit.id = i as u32;
        }
        game.next_save_unit_id = game.save_paths.len() as u32;

        info!(
            target: "rgsm::updater",
            "Assigned stable IDs to {} save units for game '{}'",
            game.save_paths.len(),
            game.name
        );
    }

    config
}

/// Backfill `storage_key` for games that were created before v1.9.0.
///
/// Strategy: for each legacy game (empty `storage_key`), if a backup directory
/// already exists with the game's display name, adopt that name as the storage
/// key to avoid renaming any directories. If the directory name would collide
/// (case-insensitive) with another game's key, generate a unique suffix.
///
/// Also replaces a legacy quick-action display-name reference with the storage
/// key assigned to that game.
fn migrate_storage_keys(mut config: Config, backup_path: &Path) -> Result<Config, UpdaterError> {
    // Resolve the old reference before newly generated keys can shadow names.
    let quick_action_index = config
        .quick_action
        .quick_action_game_id
        .as_deref()
        .and_then(|identity| config.position_game_by_identity(identity));
    let mut assigned: std::collections::HashSet<String> = config
        .games
        .iter()
        .filter(|g| !g.storage_key.is_empty())
        .map(|g| g.storage_key.clone())
        .collect();

    for game in &mut config.games {
        if !game.storage_key.is_empty() {
            continue;
        }

        // Prefer the existing directory name (which equals game.name for legacy data).
        // If a directory exists under that name, adopt it as-is to avoid renames,
        // but only when the name is safe as a single directory name.
        let legacy_dir = backup_path.join(&game.name);
        let candidate = if crate::backup::storage_key::is_safe_directory_name(&game.name)
            && legacy_dir.exists()
        {
            game.name.clone()
        } else {
            crate::backup::storage_key::generate_storage_key(&game.name)
        };

        let key = if assigned.iter().any(|k| k.eq_ignore_ascii_case(&candidate)) {
            generate_unique_storage_key(&game.name, &assigned)
        } else {
            candidate
        };

        info!(
            target: "rgsm::updater",
            "Assigned storage_key '{}' to game '{}'",
            key, game.name
        );
        if key != game.name {
            move_legacy_backup_directory(backup_path, &game.name, &key)?;
        }
        game.storage_key = key.clone();
        assigned.insert(key);
    }

    config.quick_action.quick_action_game_id =
        quick_action_index.map(|index| config.games[index].storage_key.clone());
    config.bind_legacy_game_references();

    Ok(config)
}

/// A legacy display name that cannot be a storage key (for example `a/b`)
/// still names the directory holding that game's history. Move it under the
/// new key so the history stays visible; names that could leave the backup
/// root (.., absolute paths) are never touched.
fn move_legacy_backup_directory(
    backup_path: &Path,
    legacy_name: &str,
    key: &str,
) -> std::io::Result<()> {
    let legacy_dir = backup_path.join(legacy_name);
    let target = backup_path.join(key);
    let stays_inside = Path::new(legacy_name)
        .components()
        .all(|component| matches!(component, std::path::Component::Normal(_)));
    if !stays_inside || !legacy_dir.is_dir() || target.exists() {
        return Ok(());
    }
    fs::rename(&legacy_dir, &target)?;
    info!(
        target: "rgsm::updater",
        "Moved backup directory '{}' to '{}'",
        legacy_dir.display(),
        target.display()
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{LudusaviMeta, SaveUnit, SaveUnitType, TIMER_AUTO_BACKUP_DESCRIPTION};
    use crate::device::Device;
    use crate::path_pattern::StoreKind;

    #[test]
    fn migrate_legacy_cloud_sync_inherits_true_and_preserves_explicit_false() {
        let mut config = Config::default();
        config.games.push(crate::backup::Game {
            name: "LegacyInherited".to_string(),
            storage_key: String::new(),
            save_paths: vec![],
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: false,
            auto_backup: None,
            auto_backup_limit: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        });
        config.games.push(crate::backup::Game {
            name: "ExplicitFalse".to_string(),
            storage_key: String::new(),
            save_paths: vec![],
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: false,
            auto_backup: None,
            auto_backup_limit: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        });

        let mut raw = serde_json::to_value(&config).unwrap();
        raw.pointer_mut("/settings/cloud_settings")
            .and_then(serde_json::Value::as_object_mut)
            .expect("cloud_settings should be an object")
            .insert("always_sync".to_string(), serde_json::Value::Bool(true));
        let games = raw
            .get_mut("games")
            .and_then(serde_json::Value::as_array_mut)
            .expect("games should be an array");
        games[0]
            .as_object_mut()
            .expect("game should be an object")
            .remove("cloud_sync_enabled");

        let migrated =
            migrate_legacy_cloud_sync(&serde_json::to_string(&raw).unwrap(), config).unwrap();

        assert!(migrated.games[0].cloud_sync_enabled);
        assert!(!migrated.games[1].cloud_sync_enabled);
    }

    #[test]
    fn migrate_legacy_cloud_sync_inherits_false_when_field_missing() {
        let mut config = Config::default();
        config.games.push(crate::backup::Game {
            name: "LegacyDisabled".to_string(),
            storage_key: String::new(),
            save_paths: vec![],
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            auto_backup_limit: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        });

        let mut raw = serde_json::to_value(&config).unwrap();
        raw.pointer_mut("/settings/cloud_settings")
            .and_then(serde_json::Value::as_object_mut)
            .expect("cloud_settings should be an object")
            .insert("always_sync".to_string(), serde_json::Value::Bool(false));
        raw.get_mut("games")
            .and_then(serde_json::Value::as_array_mut)
            .and_then(|games| games.first_mut())
            .and_then(serde_json::Value::as_object_mut)
            .expect("game should be an object")
            .remove("cloud_sync_enabled");

        let migrated =
            migrate_legacy_cloud_sync(&serde_json::to_string(&raw).unwrap(), config).unwrap();

        assert!(!migrated.games[0].cloud_sync_enabled);
    }

    #[test]
    fn current_config_serialization_omits_legacy_always_sync() {
        let raw = serde_json::to_value(Config::default()).unwrap();
        assert!(
            raw.pointer("/settings/cloud_settings/always_sync")
                .is_none()
        );
    }

    #[test]
    fn migrate_save_unit_ids_assigns_sequential_ids() {
        let mut config = Config::default();
        config.games.push(crate::backup::Game {
            name: "TestGame".to_string(),
            storage_key: String::new(),
            save_paths: vec![
                SaveUnit {
                    id: 0,
                    source: crate::backup::SaveUnitSource::Concrete {
                        unit_type: SaveUnitType::Folder,
                        paths: Default::default(),
                    },
                    delete_before_apply: false,
                    enabled: true,
                },
                SaveUnit {
                    id: 0,
                    source: crate::backup::SaveUnitSource::Concrete {
                        unit_type: SaveUnitType::File,
                        paths: Default::default(),
                    },
                    delete_before_apply: false,
                    enabled: true,
                },
            ],
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            auto_backup_limit: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        });

        let migrated = migrate_save_unit_ids(config);
        let game = &migrated.games[0];
        assert_eq!(game.save_paths[0].id, 0);
        assert_eq!(game.save_paths[1].id, 1);
        assert_eq!(game.next_save_unit_id, 2);
    }

    #[test]
    fn migrate_save_unit_ids_skips_already_migrated() {
        let mut config = Config::default();
        config.games.push(crate::backup::Game {
            name: "AlreadyMigrated".to_string(),
            storage_key: String::new(),
            save_paths: vec![SaveUnit {
                id: 5,
                source: crate::backup::SaveUnitSource::Concrete {
                    unit_type: SaveUnitType::File,
                    paths: Default::default(),
                },
                delete_before_apply: false,
                enabled: true,
            }],
            game_paths: Default::default(),
            next_save_unit_id: 6,
            cloud_sync_enabled: true,
            auto_backup: None,
            auto_backup_limit: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        });

        let migrated = migrate_save_unit_ids(config);
        let game = &migrated.games[0];
        assert_eq!(game.save_paths[0].id, 5);
        assert_eq!(game.next_save_unit_id, 6);
    }

    #[test]
    fn migrate_save_unit_ids_skips_empty_games() {
        let mut config = Config::default();
        config.games.push(crate::backup::Game {
            name: "EmptyGame".to_string(),
            storage_key: String::new(),
            save_paths: vec![],
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            auto_backup_limit: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        });

        let migrated = migrate_save_unit_ids(config);
        assert_eq!(migrated.games[0].next_save_unit_id, 0);
    }

    #[test]
    fn migrate_snapshot_created_by_fields_sets_missing_values_and_preserves_explicit_ones() {
        let mut raw = serde_json::json!({
            "name": "test-game",
            "backups": [
                {
                    "date": "2025-01-01_00-00-00",
                    "describe": TIMER_AUTO_BACKUP_DESCRIPTION,
                    "path": "timer.zip",
                    "size": 100
                },
                {
                    "date": "2025-01-02_00-00-00",
                    "describe": "Manual Save",
                    "path": "manual.zip",
                    "size": 100
                },
                {
                    "date": "2025-01-03_00-00-00",
                    "describe": TIMER_AUTO_BACKUP_DESCRIPTION,
                    "path": "kept.zip",
                    "size": 100,
                    "created_by": "Manual"
                }
            ]
        });

        let migrated = migrate_snapshot_created_by_fields(&mut raw);

        assert_eq!(migrated, 2);
        assert_eq!(
            raw.pointer("/backups/0/created_by").and_then(Value::as_str),
            Some("Timer")
        );
        assert_eq!(
            raw.pointer("/backups/1/created_by").and_then(Value::as_str),
            Some("Manual")
        );
        assert_eq!(
            raw.pointer("/backups/2/created_by").and_then(Value::as_str),
            Some("Manual")
        );
    }

    #[test]
    fn migrate_snapshot_created_by_in_backup_root_updates_backups_json()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = temp_dir::TempDir::new()?;
        let backup_root = temp_dir.path().join("backup");
        let game_dir = backup_root.join("migration-test");
        fs::create_dir_all(&game_dir)?;

        let raw = serde_json::json!({
            "name": "migration-test",
            "backups": [
                {
                    "date": "2025-01-01_00-00-00",
                    "describe": TIMER_AUTO_BACKUP_DESCRIPTION,
                    "path": "timer.zip",
                    "size": 100
                },
                {
                    "date": "2025-01-02_00-00-00",
                    "describe": TIMER_AUTO_BACKUP_DESCRIPTION,
                    "path": "kept.zip",
                    "size": 100,
                    "created_by": "Manual"
                }
            ],
            "device_heads": {}
        });
        fs::write(
            game_dir.join("Backups.json"),
            serde_json::to_string_pretty(&raw)?,
        )?;

        migrate_snapshot_created_by_in_backup_root(&backup_root)?;

        let migrated: Value =
            serde_json::from_str(&fs::read_to_string(game_dir.join("Backups.json"))?)?;
        assert_eq!(
            migrated
                .pointer("/backups/0/created_by")
                .and_then(Value::as_str),
            Some("Timer")
        );
        assert_eq!(
            migrated
                .pointer("/backups/1/created_by")
                .and_then(Value::as_str),
            Some("Manual")
        );

        Ok(())
    }

    #[test]
    fn update_config_too_new_error_includes_path_and_versions()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = temp_dir::TempDir::new()?;
        let config_path = temp_dir.path().join("GameSaveManager.config.json");
        fs::write(
            &config_path,
            serde_json::json!({ "version": "999.0.0" }).to_string(),
        )?;

        let err = update_config(&config_path).expect_err("version-too-new config should fail");
        match &err {
            UpdaterError::ConfigVersionTooNew {
                path,
                found,
                current,
            } => {
                assert_eq!(path, &config_path);
                assert_eq!(found, &Version::parse("999.0.0")?);
                assert_eq!(current, &Version::parse(CURRENT_VERSION)?);
            }
            other => panic!("expected ConfigVersionTooNew, got {other:?}"),
        }

        let message = err.to_string();
        assert!(message.contains(config_path.to_string_lossy().as_ref()));
        assert!(message.contains("999.0.0"));
        assert!(message.contains(CURRENT_VERSION));

        Ok(())
    }

    #[test]
    fn update_config_reports_no_migration_for_current_config()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = temp_dir::TempDir::new()?;
        let config_path = temp_dir.path().join("GameSaveManager.config.json");
        let config = Config {
            version: VERSION_1_9_0.to_string(),
            ..Default::default()
        };
        fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;

        let migrated = update_config(&config_path)?;

        assert!(!migrated);
        Ok(())
    }

    #[test]
    fn current_version_legacy_paths_migrate_to_resources_and_bindings()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = temp_dir::TempDir::new()?;
        let config_path = temp_dir.path().join("GameSaveManager.config.json");
        let device_id = get_current_device_id().clone();
        let mut config = Config::default();
        config.devices.insert(
            device_id.clone(),
            Device {
                id: device_id.clone(),
                name: "Test Device".to_string(),
                resources: Vec::new(),
                path_variables: Default::default(),
                next_resource_id: 0,
            },
        );
        config.games.push(crate::backup::Game {
            name: "Legacy Game".to_string(),
            storage_key: "legacy-game".to_string(),
            save_paths: vec![SaveUnit::concrete(
                4,
                SaveUnitType::Folder,
                std::collections::HashMap::from([(
                    device_id.clone(),
                    "<home>/Legacy Game/**/*.sav".to_string(),
                )]),
                false,
                true,
            )],
            game_paths: std::collections::HashMap::from([(
                device_id.clone(),
                "D:/Games/Legacy Game/game.exe".to_string(),
            )]),
            next_save_unit_id: 7,
            cloud_sync_enabled: true,
            auto_backup: None,
            auto_backup_limit: None,
            ludusavi_meta: Some(LudusaviMeta {
                install_dirs: vec!["Legacy Game".to_string()],
                store_game_ids: Vec::new(),
            }),
            device_bindings: std::collections::HashMap::new(),
        });

        let mut raw = serde_json::to_value(&config)?;
        raw.pointer_mut(&format!("/devices/{device_id}"))
            .and_then(Value::as_object_mut)
            .unwrap()
            .insert("game_roots".to_string(), serde_json::json!(["D:/Games"]));
        let raw_game = raw
            .pointer_mut("/games/0")
            .and_then(Value::as_object_mut)
            .unwrap();
        let raw_unit = raw_game
            .get_mut("save_paths")
            .and_then(Value::as_array_mut)
            .and_then(|units| units.first_mut())
            .and_then(Value::as_object_mut)
            .unwrap();
        let mut source = raw_unit
            .remove("source")
            .and_then(|source| source.as_object().cloned())
            .unwrap();
        raw_unit.insert("unit_type".to_string(), source.remove("unit_type").unwrap());
        raw_unit.insert("paths".to_string(), source.remove("paths").unwrap());
        raw_game.insert(
            "store_user_ids".to_string(),
            serde_json::json!({ device_id.clone(): "12345678" }),
        );
        raw_game
            .get_mut("ludusavi_meta")
            .and_then(Value::as_object_mut)
            .unwrap()
            .insert("steamId".to_string(), serde_json::json!(12345));
        fs::write(&config_path, serde_json::to_string_pretty(&raw)?)?;

        assert!(update_config(&config_path)?);
        assert!(config_path.with_extension("json.bak").exists());
        let migrated_text = fs::read_to_string(&config_path)?;
        let migrated_raw: Value = serde_json::from_str(&migrated_text)?;
        assert!(
            migrated_raw
                .pointer(&format!("/devices/{device_id}/game_roots"))
                .is_none()
        );
        assert!(migrated_raw.pointer("/games/0/store_user_ids").is_none());
        assert!(
            migrated_raw
                .pointer("/games/0/ludusavi_meta/steamId")
                .is_none()
        );

        let migrated: Config = serde_json::from_str(&migrated_text)?;
        let device = &migrated.devices[&device_id];
        assert_eq!(device.resources.len(), 2);
        assert_eq!(device.next_resource_id, 2);
        let binding = &migrated.games[0].device_bindings[&device_id];
        assert_eq!(
            binding.account_ids.as_deref(),
            Some(["resource:1".to_string()].as_slice())
        );
        assert_eq!(
            migrated.games[0]
                .ludusavi_meta
                .as_ref()
                .unwrap()
                .store_game_id(StoreKind::Steam),
            Some("12345")
        );
        assert_eq!(
            migrated.games[0].game_paths[&device_id],
            "D:/Games/Legacy Game/game.exe"
        );
        assert_eq!(migrated.games[0].save_paths[0].id, 4);
        assert_eq!(
            migrated.games[0].save_paths[0]
                .manifest_pattern()
                .unwrap()
                .0
                .raw(),
            "<home>/Legacy Game/**/*.sav"
        );
        assert_eq!(migrated.games[0].next_save_unit_id, 7);
        assert!(!update_config(&config_path)?);
        Ok(())
    }

    #[test]
    fn update_config_reports_migration_when_version_changes()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = temp_dir::TempDir::new()?;
        let config_path = temp_dir.path().join("GameSaveManager.config.json");
        let backup_path = temp_dir.path().join("backup");
        let mut config = Config {
            version: VERSION_1_8_1.to_string(),
            backup_path: backup_path.to_string_lossy().to_string(),
            ..Config::default()
        };
        config.settings.cloud_settings.backend = crate::cloud_sync::Backend::Disabled;
        fs::write(&config_path, serde_json::to_string_pretty(&config)?)?;

        let migrated = update_config(&config_path)?;

        assert!(migrated);
        let migrated_config: Config = serde_json::from_str(&fs::read_to_string(&config_path)?)?;
        assert_eq!(migrated_config.version, CURRENT_VERSION);
        Ok(())
    }

    #[test]
    fn migrate_storage_keys_backfills_empty_keys() {
        let temp_dir = temp_dir::TempDir::new().unwrap();
        let backup_path = temp_dir.path().to_path_buf();

        let mut config = Config::default();
        config.games.push(crate::backup::Game {
            name: "Normal Game".to_string(),
            storage_key: String::new(),
            save_paths: vec![],
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            auto_backup_limit: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        });
        config.games.push(crate::backup::Game {
            name: "Game: With Colon".to_string(),
            storage_key: String::new(),
            save_paths: vec![],
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            auto_backup_limit: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        });

        let migrated = migrate_storage_keys(config, &backup_path).unwrap();
        assert!(!migrated.games[0].storage_key.is_empty());
        assert!(!migrated.games[1].storage_key.is_empty());
    }

    #[test]
    fn migrate_storage_keys_adopts_existing_dir_name() {
        let temp_dir = temp_dir::TempDir::new().unwrap();
        let backup_path = temp_dir.path().to_path_buf();
        fs::create_dir(backup_path.join("My Game")).unwrap();

        let mut config = Config::default();
        config.games.push(crate::backup::Game {
            name: "My Game".to_string(),
            storage_key: String::new(),
            save_paths: vec![],
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            auto_backup_limit: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        });

        let migrated = migrate_storage_keys(config, &backup_path).unwrap();
        assert_eq!(migrated.games[0].storage_key, "My Game");
    }

    fn legacy_game(name: &str) -> crate::backup::Game {
        crate::backup::Game {
            name: name.to_string(),
            storage_key: String::new(),
            save_paths: vec![],
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            auto_backup_limit: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn migrate_storage_keys_keeps_history_of_names_that_are_not_valid_keys() {
        let temp_dir = temp_dir::TempDir::new().unwrap();
        let backup_path = temp_dir.path().join("backups");
        fs::create_dir_all(backup_path.join("a").join("b")).unwrap();
        fs::write(backup_path.join("a").join("b").join("Backups.json"), b"{}").unwrap();

        let mut config = Config::default();
        config.games.push(legacy_game("a/b"));
        config.games.push(legacy_game(".."));

        let migrated = migrate_storage_keys(config, &backup_path).unwrap();

        assert_eq!(migrated.games[0].storage_key, "a_b");
        assert!(backup_path.join("a_b").join("Backups.json").is_file());
        assert_eq!(migrated.games[1].storage_key, "_unnamed");
        assert!(backup_path.is_dir(), ".. must never move the backup root");
    }

    #[test]
    fn migrate_storage_keys_adopts_directories_that_only_repeat_underscores() {
        let temp_dir = temp_dir::TempDir::new().unwrap();
        let backup_path = temp_dir.path().to_path_buf();
        fs::create_dir(backup_path.join("My__Game")).unwrap();

        let mut config = Config::default();
        config.games.push(legacy_game("My__Game"));

        let migrated = migrate_storage_keys(config, &backup_path).unwrap();

        assert_eq!(migrated.games[0].storage_key, "My__Game");
        assert!(backup_path.join("My__Game").is_dir());
    }
    #[test]
    fn migrate_storage_keys_skips_already_populated() {
        let temp_dir = temp_dir::TempDir::new().unwrap();
        let backup_path = temp_dir.path().to_path_buf();

        let mut config = Config::default();
        config.games.push(crate::backup::Game {
            name: "Some Game".to_string(),
            storage_key: "existing_key".to_string(),
            save_paths: vec![],
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            auto_backup_limit: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        });

        let migrated = migrate_storage_keys(config, &backup_path).unwrap();
        assert_eq!(migrated.games[0].storage_key, "existing_key");
    }

    #[test]
    fn migrate_storage_keys_handles_collision() {
        let temp_dir = temp_dir::TempDir::new().unwrap();
        let backup_path = temp_dir.path().to_path_buf();

        let mut config = Config::default();
        config.games.push(crate::backup::Game {
            name: "CON".to_string(),
            storage_key: String::new(),
            save_paths: vec![],
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            auto_backup_limit: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        });
        config.games.push(crate::backup::Game {
            name: "CON".to_string(),
            storage_key: String::new(),
            save_paths: vec![],
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            auto_backup_limit: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        });

        let migrated = migrate_storage_keys(config, &backup_path).unwrap();
        let k0 = &migrated.games[0].storage_key;
        let k1 = &migrated.games[1].storage_key;
        assert_ne!(k0, k1);
        assert!(!k0.is_empty());
        assert!(!k1.is_empty());
    }

    #[test]
    fn migrate_storage_keys_preserves_quick_action_before_generated_keys_shadow_names() {
        let temp_dir = temp_dir::TempDir::new().unwrap();
        let mut config = Config::default();
        for name in ["A__B", "A_B"] {
            config.games.push(crate::backup::Game {
                name: name.to_string(),
                storage_key: String::new(),
                save_paths: vec![],
                game_paths: Default::default(),
                next_save_unit_id: 0,
                cloud_sync_enabled: false,
                auto_backup: None,
                auto_backup_limit: None,
                ludusavi_meta: None,
                device_bindings: Default::default(),
            });
        }
        config.quick_action = serde_json::from_value(serde_json::json!({
            "quick_action_game": config.games[1],
        }))
        .unwrap();

        let migrated = migrate_storage_keys(config, temp_dir.path()).unwrap();

        assert_eq!(migrated.games[0].storage_key, "A_B");
        assert_eq!(migrated.games[1].storage_key, "A_B_2");
        assert_eq!(
            migrated.quick_action.quick_action_game_id.as_deref(),
            Some("A_B_2")
        );
        assert_eq!(
            migrated
                .quick_action
                .selected_game(&migrated.games)
                .unwrap()
                .name,
            "A_B"
        );
    }

    #[test]
    fn migrate_storage_keys_backfills_quick_action_game() {
        let temp_dir = temp_dir::TempDir::new().unwrap();
        let backup_path = temp_dir.path().to_path_buf();

        let mut config = Config::default();
        config.games.push(crate::backup::Game {
            name: "QA Game".to_string(),
            storage_key: String::new(),
            save_paths: vec![],
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            auto_backup_limit: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        });
        config.quick_action.quick_action_game_id = Some("QA Game".to_string());

        let migrated = migrate_storage_keys(config, &backup_path).unwrap();
        assert_eq!(
            migrated.quick_action.quick_action_game_id.as_deref(),
            Some(migrated.games[0].storage_key.as_str())
        );
    }

    /// Restores write access when dropped so the temp dir can be removed.
    struct Locked(PathBuf);

    impl Drop for Locked {
        fn drop(&mut self) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
            }
            // Clearing the read-only attribute is Windows-only here; the Unix lint is about mode bits.
            #[cfg(windows)]
            #[allow(clippy::permissions_set_readonly_false)]
            if let Ok(metadata) = fs::metadata(&self.0) {
                let mut permissions = metadata.permissions();
                permissions.set_readonly(false);
                let _ = fs::set_permissions(&self.0, permissions);
            }
        }
    }

    /// Makes the next atomic rewrite of `json` fail: Windows refuses to replace
    /// a read-only file, Unix refuses to create the temp file in a read-only directory.
    fn block_rewrite(json: &Path) -> Locked {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir = json.parent().unwrap();
            fs::set_permissions(dir, fs::Permissions::from_mode(0o555)).unwrap();
            Locked(dir.to_path_buf())
        }
        #[cfg(windows)]
        {
            let mut permissions = fs::metadata(json).unwrap().permissions();
            permissions.set_readonly(true);
            fs::set_permissions(json, permissions).unwrap();
            Locked(json.to_path_buf())
        }
    }

    /// The `.bak` already holds these bytes, so only the rewrite itself can fail.
    fn legacy_backups_json(game_dir: &Path, raw: &Value) -> PathBuf {
        let json = game_dir.join("Backups.json");
        let bytes = serde_json::to_string_pretty(raw).unwrap();
        fs::create_dir_all(game_dir).unwrap();
        fs::write(&json, &bytes).unwrap();
        fs::write(game_dir.join("Backups.json.bak"), &bytes).unwrap();
        json
    }

    #[test]
    fn snapshot_created_by_migration_fails_when_backups_json_cannot_be_rewritten() {
        let temp_dir = temp_dir::TempDir::new().unwrap();
        let backup_root = temp_dir.path().join("backup");
        let json = legacy_backups_json(
            &backup_root.join("game"),
            &serde_json::json!({
                "name": "game",
                "backups": [{ "date": "2025-01-01_00-00-00", "describe": "x", "path": "a.zip", "size": 1 }],
                "device_heads": {}
            }),
        );
        let _locked = block_rewrite(&json);

        assert!(migrate_snapshot_created_by_in_backup_root(&backup_root).is_err());
    }

    #[test]
    fn snapshot_chain_migration_fails_when_backups_json_cannot_be_rewritten() {
        let temp_dir = temp_dir::TempDir::new().unwrap();
        let backup_root = temp_dir.path().join("backup");
        let json = legacy_backups_json(
            &backup_root.join("game"),
            &serde_json::json!({
                "name": "game",
                "backups": [
                    { "date": "2025-01-01_00-00-00", "describe": "x", "path": "a.zip", "size": 1 },
                    { "date": "2025-01-02_00-00-00", "describe": "x", "path": "b.zip", "size": 1 }
                ],
                "device_heads": {}
            }),
        );
        let _locked = block_rewrite(&json);

        assert!(migrate_game_snapshots_to_chain(&backup_root).is_err());
    }

    #[test]
    fn migrate_storage_keys_fails_when_the_history_directory_cannot_be_moved() {
        let temp_dir = temp_dir::TempDir::new().unwrap();
        let backup_path = temp_dir.path().join("backups");
        let history = backup_path.join("a").join("b");
        fs::create_dir_all(&history).unwrap();
        // Windows cannot rename a directory while a file inside it is open.
        #[cfg(windows)]
        let _held = fs::File::create(history.join("Backups.json")).unwrap();
        #[cfg(unix)]
        let _locked = {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&backup_path, fs::Permissions::from_mode(0o555)).unwrap();
            Locked(backup_path.clone())
        };
        let mut config = Config::default();
        config.games.push(legacy_game("a/b"));

        assert!(migrate_storage_keys(config, &backup_path).is_err());
    }
}
