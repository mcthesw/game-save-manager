use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::app_dirs::get_app_data_dir;
use crate::device::{DeviceId, encode_device_id, get_current_device_id};

use super::{
    CloudNamespaceGeneration, Config, ConfigurationOwners, DeviceProfile, LocalState,
    OwnershipError, SharedLibrary, V2_CONFIG_SCHEMA_VERSION,
};

pub(crate) const OWNER_DIRECTORY_NAME: &str = "GameSaveManager.config.v2";
pub(crate) const OWNER_STAGING_DIRECTORY_NAME: &str = "GameSaveManager.config.v2.staging";
pub(crate) const OWNER_ROLLBACK_DIRECTORY_NAME: &str = "GameSaveManager.config.v2.rollback";
const SHARED_LIBRARY_FILE_NAME: &str = "shared-library.json";
const LOCAL_STATE_FILE_NAME: &str = "local-state.json";
const DEVICE_PROFILES_DIRECTORY_NAME: &str = "device-profiles";
const PENDING_COMMIT_FILE_NAME: &str = ".pending-commit";

#[derive(Debug, Error)]
pub enum OwnerStoreError {
    #[error("Owner store I/O error: {0:#?}")]
    Io(#[from] std::io::Error),
    #[error("Owner store serialization error: {0:#?}")]
    Serialization(#[from] serde_json::Error),
    #[error(transparent)]
    Ownership(#[from] OwnershipError),
    #[error("Duplicate Device Profile identity: {0}")]
    DuplicateDeviceProfile(DeviceId),
    #[error("Device Profile file name does not match embedded Device ID: {0}")]
    ProfileFileNameMismatch(DeviceId),
    #[error("Local configuration changed while Cloud Library creation was in progress")]
    ActivationInputsChanged,
    #[error("Local configuration changed while Cloud Library join was in progress")]
    JoinInputsChanged,
    #[error("Local configuration changed while Cloud Cutover was in progress")]
    CutoverInputsChanged,
    #[error("The current Device Profile changed while it was being saved")]
    ProfileInputsChanged,
    #[error("The Shared Library changed while it was being saved")]
    SharedLibraryInputsChanged,
    #[error("The current Device Profile cannot remove itself")]
    CurrentProfileRemoval,
}

pub(crate) struct OwnerStore {
    root: PathBuf,
}

impl OwnerStore {
    pub(crate) fn runtime() -> Self {
        Self::new(get_app_data_dir().clone())
    }

    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub(crate) fn has_authoritative_state(&self) -> bool {
        self.active_path().exists() || self.rollback_path().exists()
    }

    pub(crate) fn initialize_from_legacy(&self, config: &Config) -> Result<bool, OwnerStoreError> {
        if self.has_authoritative_state() {
            self.load()?;
            return Ok(false);
        }
        let owners = ConfigurationOwners::from_legacy(config, get_current_device_id());
        self.write(&owners)?;
        Ok(true)
    }

    pub(crate) fn load_effective(&self) -> Result<Config, OwnerStoreError> {
        self.load()?.assemble_effective().map_err(Into::into)
    }

    pub(crate) fn merge_effective(&self, config: &Config) -> Result<Config, OwnerStoreError> {
        let mut owners = if self.has_authoritative_state() {
            self.load()?
        } else {
            ConfigurationOwners::from_legacy(config, get_current_device_id())
        };
        owners.merge_effective(config)?;
        let effective = owners.assemble_effective()?;
        self.write(&owners)?;
        Ok(effective)
    }

    pub(crate) fn replace_effective(&self, config: &Config) -> Result<Config, OwnerStoreError> {
        let existing = if self.has_authoritative_state() {
            Some(self.load()?)
        } else {
            None
        };
        let current_device_id = existing.as_ref().map_or_else(
            || get_current_device_id().clone(),
            |owners| owners.local_state.current_device_id.clone(),
        );
        let mut owners = ConfigurationOwners::from_legacy(config, &current_device_id);
        if let Some(existing) = existing {
            owners.local_state.cloud_namespace_generation =
                existing.local_state.cloud_namespace_generation;
            if let Some(existing_profile) = existing.device_profiles.get(&current_device_id)
                && let Some(current_profile) = owners.device_profiles.get_mut(&current_device_id)
            {
                current_profile.local_archive_root = existing_profile.local_archive_root.clone();
            }
        }
        let effective = owners.assemble_effective()?;
        self.write(&owners)?;
        Ok(effective)
    }

    pub(crate) fn load(&self) -> Result<ConfigurationOwners, OwnerStoreError> {
        self.recover()?;
        let active = self.active_path();
        let shared_library: SharedLibrary = read_json(&active.join(SHARED_LIBRARY_FILE_NAME))?;
        let local_state: LocalState = read_json(&active.join(LOCAL_STATE_FILE_NAME))?;
        let profiles_path = active.join(DEVICE_PROFILES_DIRECTORY_NAME);
        let mut device_profiles = HashMap::new();
        let mut profile_file_names = HashMap::new();
        for entry in fs::read_dir(profiles_path)? {
            let entry = entry?;
            if !entry.file_type()?.is_file()
                || entry.path().extension().and_then(|value| value.to_str()) != Some("json")
            {
                continue;
            }
            let profile: DeviceProfile = read_json(&entry.path())?;
            let device_id = profile.device.id.clone();
            if device_profiles.insert(device_id.clone(), profile).is_some() {
                return Err(OwnerStoreError::DuplicateDeviceProfile(device_id));
            }
            profile_file_names.insert(device_id, entry.file_name());
        }
        for (device_id, file_name) in profile_file_names {
            if file_name != profile_file_name(&device_id).as_str() {
                return Err(OwnerStoreError::ProfileFileNameMismatch(device_id));
            }
        }
        let owners = ConfigurationOwners {
            schema_version: V2_CONFIG_SCHEMA_VERSION,
            shared_library,
            device_profiles,
            local_state,
        };
        owners.validate()?;
        Ok(owners)
    }

    pub(crate) fn activate_v2(
        &self,
        expected_library: &SharedLibrary,
        expected_profile: &DeviceProfile,
    ) -> Result<(), OwnerStoreError> {
        let mut owners = self.load()?;
        let current_profile = owners
            .device_profiles
            .get(&owners.local_state.current_device_id)
            .ok_or_else(|| {
                OwnershipError::MissingDeviceProfile(owners.local_state.current_device_id.clone())
            })?;
        if owners.shared_library != *expected_library || current_profile != expected_profile {
            return Err(OwnerStoreError::ActivationInputsChanged);
        }
        owners.local_state.cloud_namespace_generation = CloudNamespaceGeneration::V2;
        self.write(&owners)
    }

    /// Downgrade from V2 when the cloud namespace is broken or manually
    /// deleted. This is a recovery path, not a normal transition.
    pub(crate) fn downgrade_to_legacy(&self) -> Result<(), OwnerStoreError> {
        let mut owners = self.load()?;
        owners.local_state.cloud_namespace_generation = CloudNamespaceGeneration::LegacyV1;
        self.write(&owners)
    }

    pub(crate) fn activate_join_v2(
        &self,
        expected_local_library: &SharedLibrary,
        expected_local_profile: &DeviceProfile,
        accepted_library: &SharedLibrary,
        accepted_profile: &DeviceProfile,
    ) -> Result<(), OwnerStoreError> {
        let mut owners = self.load()?;
        let current_device_id = owners.local_state.current_device_id.clone();
        let current_profile = owners
            .device_profiles
            .get(&current_device_id)
            .ok_or_else(|| OwnershipError::MissingDeviceProfile(current_device_id.clone()))?;
        if owners.local_state.cloud_namespace_generation != CloudNamespaceGeneration::LegacyV1
            || owners.shared_library != *expected_local_library
            || current_profile != expected_local_profile
            || accepted_profile.device.id != current_device_id
        {
            return Err(OwnerStoreError::JoinInputsChanged);
        }

        let accepted_ids = accepted_library
            .games
            .iter()
            .map(|game| game.storage_key.as_str())
            .collect::<std::collections::HashSet<_>>();
        for profile in owners.device_profiles.values_mut() {
            profile
                .games
                .retain(|id, _| accepted_ids.contains(id.as_str()));
        }
        owners.shared_library = accepted_library.clone();
        owners
            .device_profiles
            .insert(current_device_id, accepted_profile.clone());
        owners.local_state.cloud_namespace_generation = CloudNamespaceGeneration::V2;
        self.write(&owners)
    }

    pub(crate) fn activate_cutover_v2(
        &self,
        expected_local_library: &SharedLibrary,
        expected_local_profile: &DeviceProfile,
        accepted_library: &SharedLibrary,
        accepted_profiles: &HashMap<DeviceId, DeviceProfile>,
    ) -> Result<(), OwnerStoreError> {
        let mut owners = self.load()?;
        let current_device_id = owners.local_state.current_device_id.clone();
        let current_profile = owners
            .device_profiles
            .get(&current_device_id)
            .ok_or_else(|| OwnershipError::MissingDeviceProfile(current_device_id.clone()))?;
        if owners.local_state.cloud_namespace_generation != CloudNamespaceGeneration::LegacyV1
            || owners.shared_library != *expected_local_library
            || current_profile != expected_local_profile
            || !accepted_profiles.contains_key(&current_device_id)
        {
            return Err(OwnerStoreError::CutoverInputsChanged);
        }

        let accepted_current_profile = current_profile.for_shared_library(accepted_library);
        owners.shared_library = accepted_library.clone();
        owners.device_profiles = accepted_profiles.clone();
        owners
            .device_profiles
            .insert(current_device_id, accepted_current_profile);
        owners.local_state.cloud_namespace_generation = CloudNamespaceGeneration::V2;
        self.write(&owners)
    }

    pub(crate) fn replace_current_profile(
        &self,
        expected: &DeviceProfile,
        accepted: &DeviceProfile,
    ) -> Result<(), OwnerStoreError> {
        let mut owners = self.load()?;
        let current_device_id = owners.local_state.current_device_id.clone();
        let current = owners
            .device_profiles
            .get(&current_device_id)
            .ok_or_else(|| OwnershipError::MissingDeviceProfile(current_device_id.clone()))?;
        if owners.local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2
            || current != expected
            || accepted.device.id != current_device_id
        {
            return Err(OwnerStoreError::ProfileInputsChanged);
        }
        owners
            .device_profiles
            .insert(current_device_id, accepted.clone());
        self.write(&owners)
    }

    pub(crate) fn replace_shared_library(
        &self,
        expected: &SharedLibrary,
        accepted: &SharedLibrary,
    ) -> Result<(), OwnerStoreError> {
        let mut owners = self.load()?;
        if owners.local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2
            || owners.shared_library != *expected
        {
            return Err(OwnerStoreError::SharedLibraryInputsChanged);
        }
        accepted.validate()?;
        owners.shared_library = accepted.clone();
        self.write(&owners)
    }

    pub(crate) fn remove_device_profile(&self, device_id: &str) -> Result<(), OwnerStoreError> {
        let mut owners = self.load()?;
        if owners.local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(OwnerStoreError::ProfileInputsChanged);
        }
        if owners.local_state.current_device_id == device_id {
            return Err(OwnerStoreError::CurrentProfileRemoval);
        }
        owners.device_profiles.remove(device_id);
        self.write(&owners)
    }

    pub(crate) fn remove_shared_game(
        &self,
        game_id: &str,
        game_name: &str,
    ) -> Result<(), OwnerStoreError> {
        let mut owners = self.load()?;
        if owners.local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(OwnerStoreError::SharedLibraryInputsChanged);
        }
        let previous_game_count = owners.shared_library.games.len();
        owners
            .shared_library
            .games
            .retain(|game| game.storage_key != game_id);
        let mut changed = owners.shared_library.games.len() != previous_game_count;
        for profile in owners.device_profiles.values_mut() {
            changed |= profile.remove_game_state(game_id, game_name);
        }
        if changed {
            self.write(&owners)?;
        }
        Ok(())
    }

    fn write(&self, owners: &ConfigurationOwners) -> Result<(), OwnerStoreError> {
        owners.validate()?;
        let staging = self.staging_path();
        let active = self.active_path();
        let rollback = self.rollback_path();

        remove_dir_if_exists(&staging)?;
        write_store_tree(&staging, owners)?;

        if active.exists() {
            remove_dir_if_exists(&rollback)?;
            copy_directory(&active, &rollback)?;
            write_pending_marker(&active)?;
            if let Err(error) = replace_store_files(&active, &staging) {
                if restore_store_tree(&active, &rollback).is_ok() {
                    let _ = remove_pending_marker(&active);
                }
                let _ = remove_dir_if_exists(&staging);
                return Err(error);
            }
            remove_pending_marker(&active)?;
            remove_dir_if_exists(&rollback)?;
        } else {
            fs::create_dir_all(&active)?;
            write_pending_marker(&active)?;
            if let Err(error) = replace_store_files(&active, &staging) {
                let _ = remove_dir_if_exists(&active);
                let _ = remove_dir_if_exists(&staging);
                return Err(error);
            }
            remove_pending_marker(&active)?;
        }
        remove_dir_if_exists(&staging)?;
        Ok(())
    }

    fn recover(&self) -> Result<(), OwnerStoreError> {
        let active = self.active_path();
        let rollback = self.rollback_path();
        let staging = self.staging_path();
        if pending_marker_exists(&active) {
            if rollback.exists() {
                restore_store_tree(&active, &rollback)?;
                remove_pending_marker(&active)?;
                remove_dir_if_exists(&rollback)?;
            } else {
                remove_dir_if_exists(&active)?;
            }
        } else if !active.exists() && rollback.exists() {
            copy_directory(&rollback, &active)?;
            remove_dir_if_exists(&rollback)?;
        }
        if active.exists() {
            remove_dir_if_exists(&rollback)?;
        }
        remove_dir_if_exists(&staging)?;
        Ok(())
    }

    fn active_path(&self) -> PathBuf {
        self.root.join(OWNER_DIRECTORY_NAME)
    }

    fn staging_path(&self) -> PathBuf {
        self.root.join(OWNER_STAGING_DIRECTORY_NAME)
    }

    fn rollback_path(&self) -> PathBuf {
        self.root.join(OWNER_ROLLBACK_DIRECTORY_NAME)
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, OwnerStoreError> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), OwnerStoreError> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes)?;
    replace_file(&tmp, path)?;
    Ok(())
}

fn write_store_tree(root: &Path, owners: &ConfigurationOwners) -> Result<(), OwnerStoreError> {
    fs::create_dir_all(root.join(DEVICE_PROFILES_DIRECTORY_NAME))?;
    write_json(&root.join(SHARED_LIBRARY_FILE_NAME), &owners.shared_library)?;
    write_json(&root.join(LOCAL_STATE_FILE_NAME), &owners.local_state)?;
    for (device_id, profile) in &owners.device_profiles {
        write_json(
            &root
                .join(DEVICE_PROFILES_DIRECTORY_NAME)
                .join(profile_file_name(device_id)),
            profile,
        )?;
    }
    Ok(())
}

fn replace_store_files(active: &Path, staging: &Path) -> Result<(), OwnerStoreError> {
    replace_file(
        &staging.join(SHARED_LIBRARY_FILE_NAME),
        &active.join(SHARED_LIBRARY_FILE_NAME),
    )?;
    replace_file(
        &staging.join(LOCAL_STATE_FILE_NAME),
        &active.join(LOCAL_STATE_FILE_NAME),
    )?;
    let active_profiles = active.join(DEVICE_PROFILES_DIRECTORY_NAME);
    let staging_profiles = staging.join(DEVICE_PROFILES_DIRECTORY_NAME);
    fs::create_dir_all(&active_profiles)?;
    let mut keep = std::collections::HashSet::new();
    for entry in fs::read_dir(&staging_profiles)? {
        let name = entry?.file_name();
        keep.insert(name.clone());
        replace_file(&staging_profiles.join(&name), &active_profiles.join(&name))?;
    }
    for entry in fs::read_dir(&active_profiles)? {
        let entry = entry?;
        if !keep.contains(&entry.file_name()) {
            remove_path_if_exists(&entry.path())?;
        }
    }
    Ok(())
}

fn restore_store_tree(active: &Path, rollback: &Path) -> Result<(), OwnerStoreError> {
    replace_store_files(active, rollback)
}

fn pending_marker_path(active: &Path) -> PathBuf {
    active.join(PENDING_COMMIT_FILE_NAME)
}

fn pending_marker_exists(active: &Path) -> bool {
    pending_marker_path(active).exists()
}

fn write_pending_marker(active: &Path) -> Result<(), OwnerStoreError> {
    fs::create_dir_all(active)?;
    fs::write(pending_marker_path(active), b"pending\n")?;
    Ok(())
}

fn remove_pending_marker(active: &Path) -> Result<(), OwnerStoreError> {
    remove_path_if_exists(&pending_marker_path(active)).map_err(Into::into)
}

fn replace_file(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(source, target) {
        Ok(()) => return Ok(()),
        Err(error) if is_retryable_io(&error) || target.exists() => {}
        Err(error) => return Err(error),
    }
    match retry_io(
        || fs::rename(source, target),
        std::io::Error::from_raw_os_error(5),
    ) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(source, target).map(|_| ())?;
            let _ = fs::remove_file(source);
            Ok(())
        }
    }
}

fn retry_io<T>(
    mut operation: impl FnMut() -> Result<T, std::io::Error>,
    first: std::io::Error,
) -> Result<T, std::io::Error> {
    if !is_retryable_io(&first) {
        return Err(first);
    }
    for attempt in 0..8 {
        std::thread::sleep(std::time::Duration::from_millis(10 << attempt.min(4)));
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) if is_retryable_io(&error) => continue,
            Err(error) => return Err(error),
        }
    }
    operation()
}

fn is_retryable_io(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::PermissionDenied
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::Interrupted
    ) || error.raw_os_error() == Some(5)
        || error.raw_os_error() == Some(32)
}

fn remove_dir_if_exists(path: &Path) -> Result<(), std::io::Error> {
    if !path.exists() {
        return Ok(());
    }
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) => retry_io(|| fs::remove_dir_all(path), error),
    }
}

fn remove_path_if_exists(path: &Path) -> Result<(), std::io::Error> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        return remove_dir_if_exists(path);
    }
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) => retry_io(|| fs::remove_file(path), error),
    }
}

fn copy_directory(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if entry.file_name() == PENDING_COMMIT_FILE_NAME {
            continue;
        }
        let target_path = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&entry.path(), &target_path)?;
        } else {
            fs::copy(entry.path(), target_path)?;
        }
    }
    Ok(())
}

fn profile_file_name(device_id: &str) -> String {
    let mut encoded = encode_device_id(device_id);
    encoded.push_str(".json");
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::Game;
    use crate::device::Device;

    fn store() -> (temp_dir::TempDir, OwnerStore) {
        let root = temp_dir::TempDir::new().unwrap();
        let store = OwnerStore::new(root.path().to_path_buf());
        (root, store)
    }

    fn config(device_id: &str, backup_path: &str) -> Config {
        let mut config = Config {
            backup_path: backup_path.to_string(),
            ..Default::default()
        };
        config.devices.insert(
            device_id.to_string(),
            Device {
                id: device_id.to_string(),
                name: device_id.to_string(),
                resources: Vec::new(),
                next_resource_id: 0,
            },
        );
        config
    }

    fn add_device(config: &mut Config, device_id: &str, name: &str) {
        config.devices.insert(
            device_id.to_string(),
            Device {
                id: device_id.to_string(),
                name: name.to_string(),
                resources: Vec::new(),
                next_resource_id: 0,
            },
        );
    }

    #[test]
    fn writes_separate_owner_files_and_loads_effective_config() {
        let (_root, store) = store();
        let input = config(get_current_device_id(), "D:/Backups");

        assert!(store.initialize_from_legacy(&input).unwrap());
        let active = store.active_path();
        assert!(active.join(SHARED_LIBRARY_FILE_NAME).is_file());
        assert!(active.join(LOCAL_STATE_FILE_NAME).is_file());
        assert!(
            active
                .join(DEVICE_PROFILES_DIRECTORY_NAME)
                .join(profile_file_name(get_current_device_id()))
                .is_file()
        );
        assert_eq!(
            store.load_effective().unwrap().backup_path,
            input.backup_path
        );
    }

    #[test]
    fn rollback_directory_recovers_after_interrupted_activation() {
        let (_root, store) = store();
        let input = config(get_current_device_id(), "before");
        store.initialize_from_legacy(&input).unwrap();
        copy_directory(&store.active_path(), &store.rollback_path()).unwrap();
        fs::write(
            store.active_path().join(PENDING_COMMIT_FILE_NAME),
            b"pending\n",
        )
        .unwrap();
        fs::write(
            store.active_path().join(SHARED_LIBRARY_FILE_NAME),
            b"{broken",
        )
        .unwrap();
        fs::create_dir_all(store.staging_path()).unwrap();
        fs::write(store.staging_path().join("partial.json"), b"{}").unwrap();

        let recovered = store.load_effective().unwrap();

        assert_eq!(recovered.backup_path, "before");
        assert!(store.active_path().is_dir());
        assert!(!store.rollback_path().exists());
        assert!(!store.staging_path().exists());
        assert!(!store.active_path().join(PENDING_COMMIT_FILE_NAME).exists());
    }

    #[test]
    fn rollback_directory_recovers_when_active_directory_is_missing() {
        let (_root, store) = store();
        let input = config(get_current_device_id(), "before");
        store.initialize_from_legacy(&input).unwrap();
        copy_directory(&store.active_path(), &store.rollback_path()).unwrap();
        fs::remove_dir_all(store.active_path()).unwrap();

        let recovered = store.load_effective().unwrap();

        assert_eq!(recovered.backup_path, "before");
        assert!(store.active_path().is_dir());
        assert!(!store.rollback_path().exists());
    }

    #[test]
    fn malformed_active_store_does_not_fall_back_to_rollback() {
        let (_root, store) = store();
        let input = config(get_current_device_id(), "before");
        store.initialize_from_legacy(&input).unwrap();
        copy_directory(&store.active_path(), &store.rollback_path()).unwrap();
        fs::remove_file(store.active_path().join(SHARED_LIBRARY_FILE_NAME)).unwrap();

        let result = store.load_effective();

        assert!(result.is_err());
    }

    #[test]
    fn profile_file_names_do_not_contain_raw_device_ids() {
        assert_eq!(profile_file_name("../Deck"), "2e2e2f4465636b.json");
    }

    #[test]
    fn normal_merge_preserves_another_devices_private_profile() {
        let (_root, store) = store();
        let mut input = config(get_current_device_id(), "before");
        add_device(&mut input, "steam-deck", "Steam Deck");
        store.initialize_from_legacy(&input).unwrap();
        let mut owners = store.load().unwrap();
        owners
            .device_profiles
            .get_mut("steam-deck")
            .unwrap()
            .quick_action
            .quick_action_game_id = Some("deck-only".to_string());
        store.write(&owners).unwrap();
        let mut edited = store.load_effective().unwrap();
        edited.quick_action.quick_action_game_id = Some("current-only".to_string());

        store.merge_effective(&edited).unwrap();

        let persisted = store.load().unwrap();
        assert_eq!(
            persisted.device_profiles["steam-deck"]
                .quick_action
                .quick_action_game_id
                .as_deref(),
            Some("deck-only")
        );
    }

    #[test]
    fn replace_effective_keeps_this_device_archive_root() {
        let (_root, store) = store();
        let local = config(get_current_device_id(), "this-device-backups");
        store.initialize_from_legacy(&local).unwrap();

        let mut remote = config(get_current_device_id(), "other-device-backups");
        add_device(&mut remote, "other", "Other");
        store.replace_effective(&remote).unwrap();

        let persisted = store.load().unwrap();
        assert_eq!(
            persisted.device_profiles[get_current_device_id()]
                .local_archive_root
                .as_deref(),
            Some("this-device-backups")
        );
        assert_eq!(
            store.load_effective().unwrap().backup_path,
            "this-device-backups"
        );
    }

    #[test]
    fn shared_game_removal_cleans_every_local_profile() {
        let (_root, store) = store();
        let mut input = config(get_current_device_id(), "before");
        add_device(&mut input, "steam-deck", "Steam Deck");
        input.games.push(Game {
            name: "Example".into(),
            storage_key: "game".into(),
            save_paths: Vec::new(),
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: false,
            auto_backup: None,
            ludusavi_meta: None,
            device_bindings: Default::default(),
        });
        input.quick_action.quick_action_game_id = Some("game".into());
        store.initialize_from_legacy(&input).unwrap();
        let mut owners = store.load().unwrap();
        owners.local_state.cloud_namespace_generation = CloudNamespaceGeneration::V2;
        owners
            .device_profiles
            .get_mut("steam-deck")
            .unwrap()
            .quick_action
            .quick_action_game_id = Some("game".into());
        store.write(&owners).unwrap();

        store.remove_shared_game("game", "Example").unwrap();

        let stored = store.load().unwrap();
        assert!(stored.shared_library.games.is_empty());
        assert!(stored.device_profiles.values().all(|profile| {
            !profile.games.contains_key("game")
                && profile.quick_action.quick_action_game_id.is_none()
        }));
    }

    #[test]
    fn effective_saves_preserve_v2_activation() {
        let (_root, store) = store();
        let input = config(get_current_device_id(), "before");
        store.initialize_from_legacy(&input).unwrap();
        let owners = store.load().unwrap();
        let profile = &owners.device_profiles[get_current_device_id()];
        store.activate_v2(&owners.shared_library, profile).unwrap();

        store.merge_effective(&input).unwrap();
        store.replace_effective(&input).unwrap();

        assert_eq!(
            store.load().unwrap().local_state.cloud_namespace_generation,
            CloudNamespaceGeneration::V2
        );
    }

    #[test]
    fn cutover_activation_compare_checks_inputs_and_preserves_local_state() {
        let (_root, store) = store();
        let input = config(get_current_device_id(), "before");
        store.initialize_from_legacy(&input).unwrap();
        let before = store.load().unwrap();
        let expected_profile = before.device_profiles[get_current_device_id()].clone();
        let mut stale_profile = expected_profile.clone();
        stale_profile.device.name = "stale".into();

        assert!(matches!(
            store.activate_cutover_v2(
                &before.shared_library,
                &stale_profile,
                &before.shared_library,
                &before.device_profiles,
            ),
            Err(OwnerStoreError::CutoverInputsChanged)
        ));

        let mut accepted_profiles = before.device_profiles.clone();
        accepted_profiles
            .get_mut(get_current_device_id())
            .unwrap()
            .device
            .name = "Migrated Device".into();
        store
            .activate_cutover_v2(
                &before.shared_library,
                &expected_profile,
                &before.shared_library,
                &accepted_profiles,
            )
            .unwrap();
        let active = store.load().unwrap();
        assert_eq!(
            active.local_state.interface.locale,
            before.local_state.interface.locale
        );
        assert_eq!(
            active.local_state.cloud_namespace_generation,
            CloudNamespaceGeneration::V2
        );
        assert_eq!(
            active.device_profiles[get_current_device_id()].device.name,
            expected_profile.device.name
        );
    }

    #[test]
    fn activation_compare_ignores_device_profile_map_order() {
        let (_root, store) = store();
        let mut input = config(get_current_device_id(), "before");
        input.games.extend([
            Game {
                name: "Alpha".into(),
                storage_key: "alpha".into(),
                save_paths: Vec::new(),
                game_paths: Default::default(),
                next_save_unit_id: 0,
                cloud_sync_enabled: true,
                auto_backup: None,
                ludusavi_meta: None,
                device_bindings: Default::default(),
            },
            Game {
                name: "Beta".into(),
                storage_key: "beta".into(),
                save_paths: Vec::new(),
                game_paths: Default::default(),
                next_save_unit_id: 0,
                cloud_sync_enabled: true,
                auto_backup: None,
                ludusavi_meta: None,
                device_bindings: Default::default(),
            },
        ]);
        store.initialize_from_legacy(&input).unwrap();
        let before = store.load().unwrap();
        let stored = before.device_profiles[get_current_device_id()].clone();
        let mut reordered = stored.clone();
        let mut games = reordered.games.into_iter().collect::<Vec<_>>();
        games.sort_by(|left, right| left.0.cmp(&right.0).reverse());
        reordered.games = games.into_iter().collect();
        assert_eq!(stored, reordered);

        store
            .activate_cutover_v2(
                &before.shared_library,
                &reordered,
                &before.shared_library,
                &before.device_profiles,
            )
            .unwrap();
    }
    #[test]
    fn current_profile_replacement_is_v2_only_and_compare_checked() {
        let (_root, store) = store();
        let input = config(get_current_device_id(), "before");
        store.initialize_from_legacy(&input).unwrap();
        let owners = store.load().unwrap();
        let expected = owners.device_profiles[get_current_device_id()].clone();
        let mut accepted = expected.clone();
        accepted.device.name = "Synchronized profile".into();

        assert!(matches!(
            store.replace_current_profile(&expected, &accepted),
            Err(OwnerStoreError::ProfileInputsChanged)
        ));
        store
            .activate_v2(&owners.shared_library, &expected)
            .unwrap();
        store.replace_current_profile(&expected, &accepted).unwrap();
        assert_eq!(
            store.load().unwrap().device_profiles[get_current_device_id()]
                .device
                .name,
            "Synchronized profile"
        );
        assert!(matches!(
            store.replace_current_profile(&expected, &accepted),
            Err(OwnerStoreError::ProfileInputsChanged)
        ));
    }

    #[test]
    fn shared_library_replacement_is_v2_only_and_compare_checked() {
        let (_root, store) = store();
        let input = config(get_current_device_id(), "before");
        store.initialize_from_legacy(&input).unwrap();
        let owners = store.load().unwrap();
        let expected = owners.shared_library.clone();
        let mut accepted = expected.clone();
        accepted.games.push(crate::config::SharedGame {
            name: "Synchronized game".into(),
            storage_key: "synchronized-game".into(),
            save_units: Vec::new(),
            next_save_unit_id: 0,
            ludusavi_meta: None,
            snapshot_retention: None,
        });

        assert!(matches!(
            store.replace_shared_library(&expected, &accepted),
            Err(OwnerStoreError::SharedLibraryInputsChanged)
        ));
        store
            .activate_v2(&expected, &owners.device_profiles[get_current_device_id()])
            .unwrap();
        store.replace_shared_library(&expected, &accepted).unwrap();
        assert_eq!(
            store.load().unwrap().shared_library.games[0].name,
            "Synchronized game"
        );
        assert!(matches!(
            store.replace_shared_library(&expected, &accepted),
            Err(OwnerStoreError::SharedLibraryInputsChanged)
        ));
    }

    #[test]
    fn explicit_replacement_rebuilds_all_device_profiles() {
        let (_root, store) = store();
        let mut input = config(get_current_device_id(), "before");
        add_device(&mut input, "steam-deck", "Old Deck");
        store.initialize_from_legacy(&input).unwrap();
        input.devices.get_mut("steam-deck").unwrap().name = "Replacement Deck".to_string();

        store.replace_effective(&input).unwrap();

        assert_eq!(
            store.load_effective().unwrap().devices["steam-deck"].name,
            "Replacement Deck"
        );
    }

    #[test]
    fn unsupported_schema_fails_closed() {
        let (_root, store) = store();
        let input = config(get_current_device_id(), "before");
        store.initialize_from_legacy(&input).unwrap();
        let shared_path = store.active_path().join(SHARED_LIBRARY_FILE_NAME);
        let mut shared: serde_json::Value =
            serde_json::from_slice(&fs::read(&shared_path).unwrap()).unwrap();
        shared["schema_version"] = serde_json::json!(99);
        fs::write(shared_path, serde_json::to_vec_pretty(&shared).unwrap()).unwrap();

        let result = store.load_effective();

        assert!(matches!(
            result,
            Err(OwnerStoreError::Ownership(
                OwnershipError::UnsupportedSchema { found: 99, .. }
            ))
        ));
    }

    #[test]
    fn duplicate_device_profile_identity_is_rejected() {
        let (_root, store) = store();
        let input = config(get_current_device_id(), "before");
        store.initialize_from_legacy(&input).unwrap();
        let profiles = store.active_path().join(DEVICE_PROFILES_DIRECTORY_NAME);
        fs::copy(
            profiles.join(profile_file_name(get_current_device_id())),
            profiles.join("duplicate.json"),
        )
        .unwrap();

        let result = store.load_effective();

        assert!(matches!(
            result,
            Err(OwnerStoreError::DuplicateDeviceProfile(device_id))
                if device_id == *get_current_device_id()
        ));
    }

    fn copy_directory(source: &Path, target: &Path) -> Result<(), std::io::Error> {
        fs::create_dir_all(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let target_path = target.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                copy_directory(&entry.path(), &target_path)?;
            } else {
                fs::copy(entry.path(), target_path)?;
            }
        }
        Ok(())
    }
}
