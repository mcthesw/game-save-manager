//! Serializable V2 configuration owners and the compatibility projection used
//! while the application still consumes the legacy flat [`Config`] model.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use crate::backup::{
    AutoBackupConfig, CompressionPreset, Game, GameDeviceBinding, LudusaviMeta, SaveUnit,
    SaveUnitSource, SaveUnitType,
};
use crate::cloud_sync::CloudSettings;
use crate::device::{Device, DeviceId};
use crate::path_pattern::{ManifestPathConstraints, ManifestPathPattern};

use super::{
    AppearanceSettings, Config, FavoriteTreeNode, QuickActionsSettings, SaveListExpandBehavior,
    SaveListSortMode, Settings, SortDirection,
};

pub const V2_CONFIG_SCHEMA_VERSION: u32 = 2;
pub type EffectiveConfiguration = Config;

#[derive(Debug, Error)]
pub enum OwnershipError {
    #[error("Device Profile not found: {0}")]
    MissingDeviceProfile(DeviceId),
    #[error("Unsupported {owner} schema version: {found}")]
    UnsupportedSchema { owner: String, found: u32 },
    #[error("Device Profile key {key} does not match embedded Device ID {embedded}")]
    ProfileIdentityMismatch { key: DeviceId, embedded: DeviceId },
    #[error("Shared Library contains duplicate Game identity: {0}")]
    DuplicateSharedGame(String),
    #[error("Shared Library contains an empty Game identity")]
    EmptySharedGameId,
    #[error("Shared Game {game_id} contains duplicate Save Unit ID {save_unit_id}")]
    DuplicateSharedSaveUnit { game_id: String, save_unit_id: u32 },
    #[error("Shared Game {0} has an invalid Snapshot retention limit")]
    InvalidSnapshotRetention(String),
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct ConfigurationOwners {
    pub schema_version: u32,
    pub shared_library: SharedLibrary,
    pub device_profiles: HashMap<DeviceId, DeviceProfile>,
    pub local_state: LocalState,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, PartialEq, Eq)]
pub struct SharedLibrary {
    pub schema_version: u32,
    pub games: Vec<SharedGame>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, PartialEq, Eq)]
pub struct SharedGame {
    pub name: String,
    pub storage_key: String,
    pub save_units: Vec<SharedSaveUnit>,
    pub next_save_unit_id: u32,
    pub ludusavi_meta: Option<LudusaviMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_retention: Option<SharedSnapshotRetentionPolicy>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Type, PartialEq, Eq)]
pub struct SharedSnapshotRetentionPolicy {
    pub automatic_snapshots_per_branch: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, PartialEq, Eq)]
pub struct SharedSaveUnit {
    pub id: u32,
    pub source: SharedSaveUnitSource,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SharedSaveUnitSource {
    Concrete {
        unit_type: SaveUnitType,
    },
    ManifestPattern {
        pattern: ManifestPathPattern,
        constraints: ManifestPathConstraints,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct DeviceProfile {
    pub schema_version: u32,
    pub device: Device,
    pub local_archive_root: Option<String>,
    pub games: HashMap<String, DeviceGameProfile>,
    pub private_favorites: Vec<FavoriteTreeNode>,
    pub quick_action: QuickActionsSettings,
    pub behavior: DeviceBehaviorSettings,
}

impl DeviceProfile {
    pub(crate) fn remove_game_state(&mut self, game_id: &str, game_name: &str) -> bool {
        let game_changed = self.games.remove(game_id).is_some();
        let quick_action_changed = self.quick_action.remove_game_reference(game_id, game_name);
        let favorites_changed =
            FavoriteTreeNode::remove_game_leaves(&mut self.private_favorites, game_name);
        game_changed || quick_action_changed || favorites_changed
    }

    pub(crate) fn references_game(&self, game_id: &str, game_name: &str) -> bool {
        self.games.contains_key(game_id)
            || self
                .quick_action
                .references_game_identity(game_id, game_name)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct DeviceGameProfile {
    pub visible: bool,
    pub sync_mode: SyncMode,
    #[serde(default)]
    pub snapshot_sync_activation_revision: Option<u64>,
    #[serde(default)]
    pub snapshot_sync_local_baseline: BTreeSet<String>,
    #[serde(default)]
    pub initial_catch_up: InitialCatchUpPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_save_process_name: Option<String>,
    #[serde(default)]
    pub live_save_snapshot_on_exit: bool,
    pub game_path: Option<String>,
    pub binding: Option<GameDeviceBinding>,
    pub auto_backup: Option<AutoBackupConfig>,
    pub save_units: HashMap<u32, DeviceSaveUnitSettings>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct DeviceSaveUnitSettings {
    pub path: Option<String>,
    pub enabled: bool,
    pub delete_before_apply: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum SyncMode {
    Manual,
    SnapshotSync,
    LiveSaveSync,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum InitialCatchUpPolicy {
    #[default]
    KeepRemote,
    DownloadExisting,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum CloudNamespaceGeneration {
    #[default]
    LegacyV1,
    V2,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct DeviceBehaviorSettings {
    pub prompt_when_not_described: bool,
    pub extra_backup_when_apply: bool,
    pub confirm_before_apply_latest: bool,
    pub confirm_before_apply_snapshot: bool,
    pub prompt_when_auto_backup: bool,
    pub default_delete_before_apply: bool,
    pub add_new_to_favorites: bool,
    pub vn_scan_dirs: Vec<String>,
    pub max_auto_backup_count: u32,
    pub max_extra_backup_count: u32,
    pub compression_preset: CompressionPreset,
    pub compute_archive_hash: bool,
    pub verify_archive_before_apply: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct LocalState {
    pub schema_version: u32,
    pub legacy_version: String,
    pub current_device_id: DeviceId,
    pub interface: LocalInterfaceSettings,
    pub cloud_settings: CloudSettings,
    #[serde(default)]
    pub cloud_namespace_generation: CloudNamespaceGeneration,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct LocalInterfaceSettings {
    pub show_edit_button: bool,
    pub exit_to_tray: bool,
    pub locale: String,
    pub default_expend_favorites_tree: bool,
    pub home_page: String,
    pub log_to_file: bool,
    pub save_list_expand_behavior: SaveListExpandBehavior,
    pub save_list_last_expanded: bool,
    pub save_list_sort_mode: SaveListSortMode,
    pub save_list_sort_direction: SortDirection,
    pub appearance: AppearanceSettings,
}

impl ConfigurationOwners {
    /// Split a loaded flat configuration into explicit shared, per-device, and
    /// local owners without consulting process-global device state.
    pub fn from_legacy(config: &Config, current_device_id: &DeviceId) -> Self {
        let shared_library = SharedLibrary {
            schema_version: V2_CONFIG_SCHEMA_VERSION,
            games: config.games.iter().map(SharedGame::from).collect(),
        };
        let behavior = DeviceBehaviorSettings::from(&config.settings);
        let mut device_ids = config.devices.keys().cloned().collect::<HashSet<_>>();
        device_ids.insert(current_device_id.clone());
        for game in &config.games {
            device_ids.extend(game.game_paths.keys().cloned());
            device_ids.extend(game.device_bindings.keys().cloned());
            for save_unit in &game.save_paths {
                if let Some(paths) = save_unit.paths() {
                    device_ids.extend(paths.keys().cloned());
                }
            }
        }

        let device_profiles = device_ids
            .into_iter()
            .map(|device_id| {
                let is_current = &device_id == current_device_id;
                let device = config
                    .devices
                    .get(&device_id)
                    .cloned()
                    .unwrap_or_else(|| Device {
                        id: device_id.clone(),
                        name: device_id.clone(),
                        resources: Vec::new(),
                        next_resource_id: 0,
                    });
                let games = config
                    .games
                    .iter()
                    .map(|game| {
                        (
                            game.storage_key.clone(),
                            DeviceGameProfile::from_legacy(game, &device_id),
                        )
                    })
                    .collect();
                let profile = DeviceProfile {
                    schema_version: V2_CONFIG_SCHEMA_VERSION,
                    device,
                    local_archive_root: is_current.then(|| config.backup_path.clone()),
                    games,
                    private_favorites: if is_current {
                        config.favorites.clone()
                    } else {
                        Vec::new()
                    },
                    quick_action: if is_current {
                        config.quick_action.clone()
                    } else {
                        QuickActionsSettings::default()
                    },
                    behavior: behavior.clone(),
                };
                (device_id, profile)
            })
            .collect();

        Self {
            schema_version: V2_CONFIG_SCHEMA_VERSION,
            shared_library,
            device_profiles,
            local_state: LocalState {
                schema_version: V2_CONFIG_SCHEMA_VERSION,
                legacy_version: config.version.clone(),
                current_device_id: current_device_id.clone(),
                interface: LocalInterfaceSettings::from(&config.settings),
                cloud_settings: config.settings.cloud_settings.clone(),
                cloud_namespace_generation: CloudNamespaceGeneration::LegacyV1,
            },
        }
    }

    pub fn validate(&self) -> Result<(), OwnershipError> {
        validate_schema("Configuration Owners", self.schema_version)?;
        self.shared_library.validate()?;
        validate_schema("Local State", self.local_state.schema_version)?;

        for (device_id, profile) in &self.device_profiles {
            validate_schema(
                &format!("Device Profile {device_id}"),
                profile.schema_version,
            )?;
            if &profile.device.id != device_id {
                return Err(OwnershipError::ProfileIdentityMismatch {
                    key: device_id.clone(),
                    embedded: profile.device.id.clone(),
                });
            }
        }
        if !self
            .device_profiles
            .contains_key(&self.local_state.current_device_id)
        {
            return Err(OwnershipError::MissingDeviceProfile(
                self.local_state.current_device_id.clone(),
            ));
        }
        Ok(())
    }

    /// Apply an effective-configuration save without granting the current
    /// Device permission to rewrite another Device's private Profile fields.
    pub fn merge_effective(&mut self, config: &Config) -> Result<(), OwnershipError> {
        self.validate()?;
        let current_device_id = self.local_state.current_device_id.clone();
        let previously_shared = self
            .shared_library
            .games
            .iter()
            .map(|game| game.storage_key.clone())
            .collect::<HashSet<_>>();
        let previously_managed = self
            .device_profiles
            .get(&current_device_id)
            .ok_or_else(|| OwnershipError::MissingDeviceProfile(current_device_id.clone()))?
            .games
            .clone();
        let mut incoming = Self::from_legacy(config, &current_device_id);
        let mut current_profile = incoming
            .device_profiles
            .remove(&current_device_id)
            .ok_or_else(|| OwnershipError::MissingDeviceProfile(current_device_id.clone()))?;
        let incoming_device_ids = incoming
            .device_profiles
            .keys()
            .cloned()
            .collect::<HashSet<_>>();

        let existing_retention = self
            .shared_library
            .games
            .iter()
            .map(|game| (game.storage_key.as_str(), game.snapshot_retention))
            .collect::<HashMap<_, _>>();
        for game in &mut incoming.shared_library.games {
            game.snapshot_retention = existing_retention
                .get(game.storage_key.as_str())
                .copied()
                .flatten();
        }
        // The flat compatibility view cannot represent "not managed" or
        // Device Visibility. Preserve both for existing shared Games, while
        // still creating conservative defaults for genuinely new Games.
        current_profile.games.retain(|game_id, settings| {
            if let Some(previous) = previously_managed.get(game_id) {
                settings.visible = previous.visible;
                true
            } else {
                !previously_shared.contains(game_id)
            }
        });
        self.shared_library = incoming.shared_library;
        incoming.local_state.cloud_namespace_generation =
            self.local_state.cloud_namespace_generation;
        self.local_state = incoming.local_state;
        self.device_profiles.retain(|device_id, _| {
            device_id == &current_device_id || incoming_device_ids.contains(device_id)
        });
        for (device_id, profile) in incoming.device_profiles {
            self.device_profiles.entry(device_id).or_insert(profile);
        }
        self.device_profiles
            .insert(current_device_id, current_profile);

        let shared_game_ids = self
            .shared_library
            .games
            .iter()
            .map(|game| game.storage_key.as_str())
            .collect::<HashSet<_>>();
        for profile in self.device_profiles.values_mut() {
            profile
                .games
                .retain(|game_id, _| shared_game_ids.contains(game_id.as_str()));
        }
        self.validate()
    }

    /// Join the selected Device Profile with shared and local data for existing
    /// application services that still consume the flat configuration model.
    pub fn assemble_effective(&self) -> Result<EffectiveConfiguration, OwnershipError> {
        self.validate()?;
        let current_device_id = &self.local_state.current_device_id;
        let current = self
            .device_profiles
            .get(current_device_id)
            .ok_or_else(|| OwnershipError::MissingDeviceProfile(current_device_id.clone()))?;
        let games = self
            .shared_library
            .games
            .iter()
            .map(|shared| self.assemble_game(shared, current_device_id))
            .collect();
        let devices = self
            .device_profiles
            .iter()
            .map(|(id, profile)| (id.clone(), profile.device.clone()))
            .collect();

        Ok(Config {
            version: self.local_state.legacy_version.clone(),
            backup_path: current
                .local_archive_root
                .clone()
                .unwrap_or_else(|| "save_data".to_string()),
            games,
            settings: Settings::from_owners(&current.behavior, &self.local_state),
            favorites: current.private_favorites.clone(),
            quick_action: current.quick_action.clone(),
            devices,
        })
    }

    fn assemble_game(&self, shared: &SharedGame, current_device_id: &DeviceId) -> Game {
        let current = self
            .device_profiles
            .get(current_device_id)
            .and_then(|profile| profile.games.get(&shared.storage_key));
        let mut game_paths = HashMap::new();
        let mut device_bindings = HashMap::new();
        for (device_id, profile) in &self.device_profiles {
            if let Some(game) = profile.games.get(&shared.storage_key) {
                if let Some(path) = &game.game_path {
                    game_paths.insert(device_id.clone(), path.clone());
                }
                if let Some(binding) = &game.binding {
                    device_bindings.insert(device_id.clone(), binding.clone());
                }
            }
        }
        let save_paths = shared
            .save_units
            .iter()
            .map(|unit| {
                let current_settings = current.and_then(|game| game.save_units.get(&unit.id));
                let source = match &unit.source {
                    SharedSaveUnitSource::Concrete { unit_type } => {
                        let paths = self
                            .device_profiles
                            .iter()
                            .filter_map(|(device_id, profile)| {
                                profile
                                    .games
                                    .get(&shared.storage_key)?
                                    .save_units
                                    .get(&unit.id)?
                                    .path
                                    .as_ref()
                                    .map(|path| (device_id.clone(), path.clone()))
                            })
                            .collect();
                        SaveUnitSource::Concrete {
                            unit_type: unit_type.clone(),
                            paths,
                        }
                    }
                    SharedSaveUnitSource::ManifestPattern {
                        pattern,
                        constraints,
                    } => SaveUnitSource::ManifestPattern {
                        pattern: pattern.clone(),
                        constraints: constraints.clone(),
                    },
                };
                SaveUnit {
                    id: unit.id,
                    source,
                    delete_before_apply: current_settings
                        .is_some_and(|settings| settings.delete_before_apply),
                    enabled: current_settings.is_none_or(|settings| settings.enabled),
                }
            })
            .collect();

        Game {
            name: shared.name.clone(),
            storage_key: shared.storage_key.clone(),
            save_paths,
            game_paths,
            next_save_unit_id: shared.next_save_unit_id,
            cloud_sync_enabled: current.is_some_and(|game| game.sync_mode != SyncMode::Manual),
            auto_backup: current.and_then(|game| game.auto_backup.clone()),
            ludusavi_meta: shared.ludusavi_meta.clone(),
            device_bindings,
        }
    }
}

impl DeviceProfile {
    /// Retain only current-Device values that still apply to an accepted
    /// Shared Library, and provide conservative defaults for newly seen Games.
    pub fn for_shared_library(&self, library: &SharedLibrary) -> Self {
        let mut profile = self.clone();
        profile.games = library
            .games
            .iter()
            .map(|game| {
                let save_unit_ids = game
                    .save_units
                    .iter()
                    .map(|unit| unit.id)
                    .collect::<HashSet<_>>();
                let mut settings =
                    self.games
                        .get(&game.storage_key)
                        .cloned()
                        .unwrap_or(DeviceGameProfile {
                            visible: true,
                            sync_mode: SyncMode::Manual,
                            snapshot_sync_activation_revision: None,
                            snapshot_sync_local_baseline: BTreeSet::new(),
                            initial_catch_up: InitialCatchUpPolicy::KeepRemote,
                            live_save_process_name: None,
                            live_save_snapshot_on_exit: false,
                            game_path: None,
                            binding: None,
                            auto_backup: None,
                            save_units: HashMap::new(),
                        });
                settings
                    .save_units
                    .retain(|id, _| save_unit_ids.contains(id));
                for id in save_unit_ids {
                    settings
                        .save_units
                        .entry(id)
                        .or_insert(DeviceSaveUnitSettings {
                            path: None,
                            enabled: true,
                            delete_before_apply: false,
                        });
                }
                (game.storage_key.clone(), settings)
            })
            .collect();
        profile
    }
}

impl SharedLibrary {
    /// Validate remote-portable configuration without requiring Local State or
    /// a current Device Profile.
    pub fn validate(&self) -> Result<(), OwnershipError> {
        validate_schema("Shared Library", self.schema_version)?;
        let mut game_ids = HashSet::new();
        for game in &self.games {
            if game.storage_key.trim().is_empty() {
                return Err(OwnershipError::EmptySharedGameId);
            }
            if !game_ids.insert(&game.storage_key) {
                return Err(OwnershipError::DuplicateSharedGame(
                    game.storage_key.clone(),
                ));
            }
            let mut save_unit_ids = HashSet::new();
            for save_unit in &game.save_units {
                if !save_unit_ids.insert(save_unit.id) {
                    return Err(OwnershipError::DuplicateSharedSaveUnit {
                        game_id: game.storage_key.clone(),
                        save_unit_id: save_unit.id,
                    });
                }
            }
            if game
                .snapshot_retention
                .is_some_and(|policy| policy.automatic_snapshots_per_branch == 0)
            {
                return Err(OwnershipError::InvalidSnapshotRetention(
                    game.storage_key.clone(),
                ));
            }
        }
        Ok(())
    }
}

fn validate_schema(owner: &str, found: u32) -> Result<(), OwnershipError> {
    if found == V2_CONFIG_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(OwnershipError::UnsupportedSchema {
            owner: owner.to_string(),
            found,
        })
    }
}

impl From<&Game> for SharedGame {
    fn from(game: &Game) -> Self {
        Self {
            name: game.name.clone(),
            storage_key: game.storage_key.clone(),
            save_units: game.save_paths.iter().map(SharedSaveUnit::from).collect(),
            next_save_unit_id: game.next_save_unit_id,
            ludusavi_meta: game.ludusavi_meta.clone(),
            snapshot_retention: None,
        }
    }
}

impl From<&SaveUnit> for SharedSaveUnit {
    fn from(unit: &SaveUnit) -> Self {
        let source = match &unit.source {
            SaveUnitSource::Concrete { unit_type, .. } => SharedSaveUnitSource::Concrete {
                unit_type: unit_type.clone(),
            },
            SaveUnitSource::ManifestPattern {
                pattern,
                constraints,
            } => SharedSaveUnitSource::ManifestPattern {
                pattern: pattern.clone(),
                constraints: constraints.clone(),
            },
        };
        Self {
            id: unit.id,
            source,
        }
    }
}

impl DeviceGameProfile {
    fn from_legacy(game: &Game, device_id: &DeviceId) -> Self {
        let save_units = game
            .save_paths
            .iter()
            .map(|unit| {
                (
                    unit.id,
                    DeviceSaveUnitSettings {
                        path: unit.get_path_for_device(device_id).cloned(),
                        enabled: unit.enabled,
                        delete_before_apply: unit.delete_before_apply,
                    },
                )
            })
            .collect();
        Self {
            visible: true,
            sync_mode: if game.cloud_sync_enabled {
                SyncMode::SnapshotSync
            } else {
                SyncMode::Manual
            },
            snapshot_sync_activation_revision: game.cloud_sync_enabled.then_some(0),
            snapshot_sync_local_baseline: BTreeSet::new(),
            initial_catch_up: InitialCatchUpPolicy::KeepRemote,
            live_save_process_name: None,
            live_save_snapshot_on_exit: false,
            game_path: game.game_paths.get(device_id).cloned(),
            binding: game.device_bindings.get(device_id).cloned(),
            auto_backup: game.auto_backup.clone(),
            save_units,
        }
    }
}

impl From<&Settings> for DeviceBehaviorSettings {
    fn from(settings: &Settings) -> Self {
        Self {
            prompt_when_not_described: settings.prompt_when_not_described,
            extra_backup_when_apply: settings.extra_backup_when_apply,
            confirm_before_apply_latest: settings.confirm_before_apply_latest,
            confirm_before_apply_snapshot: settings.confirm_before_apply_snapshot,
            prompt_when_auto_backup: settings.prompt_when_auto_backup,
            default_delete_before_apply: settings.default_delete_before_apply,
            add_new_to_favorites: settings.add_new_to_favorites,
            vn_scan_dirs: settings.vn_scan_dirs.clone(),
            max_auto_backup_count: settings.max_auto_backup_count,
            max_extra_backup_count: settings.max_extra_backup_count,
            compression_preset: settings.compression_preset,
            compute_archive_hash: settings.compute_archive_hash,
            verify_archive_before_apply: settings.verify_archive_before_apply,
        }
    }
}

impl From<&Settings> for LocalInterfaceSettings {
    fn from(settings: &Settings) -> Self {
        Self {
            show_edit_button: settings.show_edit_button,
            exit_to_tray: settings.exit_to_tray,
            locale: settings.locale.clone(),
            default_expend_favorites_tree: settings.default_expend_favorites_tree,
            home_page: settings.home_page.clone(),
            log_to_file: settings.log_to_file,
            save_list_expand_behavior: settings.save_list_expand_behavior.clone(),
            save_list_last_expanded: settings.save_list_last_expanded,
            save_list_sort_mode: settings.save_list_sort_mode,
            save_list_sort_direction: settings.save_list_sort_direction,
            appearance: settings.appearance.clone(),
        }
    }
}

impl Settings {
    fn from_owners(behavior: &DeviceBehaviorSettings, local: &LocalState) -> Self {
        Self {
            prompt_when_not_described: behavior.prompt_when_not_described,
            extra_backup_when_apply: behavior.extra_backup_when_apply,
            confirm_before_apply_latest: behavior.confirm_before_apply_latest,
            confirm_before_apply_snapshot: behavior.confirm_before_apply_snapshot,
            show_edit_button: local.interface.show_edit_button,
            prompt_when_auto_backup: behavior.prompt_when_auto_backup,
            exit_to_tray: local.interface.exit_to_tray,
            cloud_settings: local.cloud_settings.clone(),
            locale: local.interface.locale.clone(),
            default_delete_before_apply: behavior.default_delete_before_apply,
            default_expend_favorites_tree: local.interface.default_expend_favorites_tree,
            home_page: local.interface.home_page.clone(),
            log_to_file: local.interface.log_to_file,
            add_new_to_favorites: behavior.add_new_to_favorites,
            vn_scan_dirs: behavior.vn_scan_dirs.clone(),
            save_list_expand_behavior: local.interface.save_list_expand_behavior.clone(),
            save_list_last_expanded: local.interface.save_list_last_expanded,
            save_list_sort_mode: local.interface.save_list_sort_mode,
            save_list_sort_direction: local.interface.save_list_sort_direction,
            max_auto_backup_count: behavior.max_auto_backup_count,
            max_extra_backup_count: behavior.max_extra_backup_count,
            appearance: local.interface.appearance.clone(),
            compression_preset: behavior.compression_preset,
            compute_archive_hash: behavior.compute_archive_hash,
            verify_archive_before_apply: behavior.verify_archive_before_apply,
        }
    }
}
