use log::{info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use crate::backup::archive::RestoreNotifier;
use crate::backup::extra_backups::cleanup_oldest_extra_backups;
use crate::backup::state_fingerprint::{
    fingerprint_source_state, fingerprint_zip_state, read_stored_fingerprint,
};
use crate::backup::{
    ArchiveBackend, ArchiveFormat, CapturePlan, CreatedBy, GameDeviceBinding, GameSnapshots,
    SaveUnit, SaveUnitDraft, SevenZBackend, Snapshot, ZipBackend, archive_file_name,
};
use crate::config::{get_backup_path, get_config, set_config_local};
use crate::device::{DeviceId, DeviceResourceKind, get_current_device_id};
use crate::path_pattern::StoreKind;
use crate::path_resolver::PathContext;
use crate::preclude::*;

/// Manifest-derived metadata for games imported from Ludusavi.
///
/// These fields are **device-independent** (directory names and IDs, not paths)
/// and are safe to persist in config and sync across devices.
#[derive(Debug, Serialize, Deserialize, Clone, Type, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LudusaviMeta {
    /// Install directory names from the manifest's `installDir` field.
    /// e.g. `["100 Orange Juice"]`. Used to resolve `<game>` and `<base>`.
    #[serde(default)]
    pub install_dirs: Vec<String>,
    /// Store-specific game IDs used to resolve `<storeGameId>` in the context
    /// of the candidate root or installation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub store_game_ids: Vec<StoreGameId>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoreGameId {
    pub store: StoreKind,
    pub id: String,
}

impl LudusaviMeta {
    pub fn store_game_id(&self, store: StoreKind) -> Option<&str> {
        self.store_game_ids
            .iter()
            .find(|entry| entry.store == store)
            .map(|entry| entry.id.as_str())
    }
}

/// Per-game auto-backup configuration.
/// Presence (`Some`) enables the timer; absence (`None`) disables it.
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct AutoBackupConfig {
    /// Interval between auto-backups in seconds.
    pub interval_secs: u32,
    /// Maximum number of auto-backups to keep. `None` = use global setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_backup_count: Option<u32>,
}

/// A game struct contains the save units and the game's launcher
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct Game {
    pub name: String,
    /// Filesystem-safe identifier used for local backup directories and remote
    /// cloud paths. Derived from `name` via [`super::storage_key::generate_storage_key`]
    /// when the game is first created. Once set it never changes, even if the
    /// display `name` is later renamed.
    #[serde(default)]
    pub storage_key: String,
    pub save_paths: Vec<SaveUnit>,
    // 使用 HashMap 存储不同设备的启动路径
    // Key: DeviceId (String), Value: Path (String)
    #[serde(default)]
    pub game_paths: HashMap<DeviceId, String>,
    /// Monotonically increasing counter for assigning unique save-unit IDs.
    /// IDs can be provided by callers (frontend/CLI/FFI), and backend normalization
    /// keeps this counter aligned with the highest in-use ID.
    #[serde(default)]
    pub next_save_unit_id: u32,
    /// Whether this game participates in cloud sync.
    /// Defaults to true so existing games are automatically included.
    #[serde(default = "crate::default_value::default_true")]
    pub cloud_sync_enabled: bool,
    /// Per-game auto-backup configuration. `None` = disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_backup: Option<AutoBackupConfig>,
    /// Metadata from Ludusavi manifest import. `None` for manually added games.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ludusavi_meta: Option<LudusaviMeta>,
    /// Per-Device resource selections for this Game. Missing dimensions remain
    /// implicit and become ambiguous if more than one applicable resource exists.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub device_bindings: HashMap<DeviceId, GameDeviceBinding>,
}

/// Frontend/IPC input shape for creating/updating a game.
/// Save-unit IDs are assigned and normalized in backend domain logic.
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct GameDraft {
    pub name: String,
    pub save_paths: Vec<SaveUnitDraft>,
    #[serde(default)]
    pub game_paths: HashMap<DeviceId, String>,
    /// Metadata from Ludusavi manifest import (optional for manually added games).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ludusavi_meta: Option<LudusaviMeta>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub device_bindings: HashMap<DeviceId, GameDeviceBinding>,
}

fn save_unit_identity(source: &crate::backup::SaveUnitSource) -> String {
    match source {
        crate::backup::SaveUnitSource::Concrete { unit_type, paths } => {
            let mut entries = paths.iter().collect::<Vec<_>>();
            entries.sort_by_key(|(device_id, _)| *device_id);
            let paths_key = entries
                .into_iter()
                .map(|(device_id, path)| format!("{device_id}={path}"))
                .collect::<Vec<_>>()
                .join("|");
            format!("concrete:{unit_type:?}|{paths_key}")
        }
        crate::backup::SaveUnitSource::ManifestPattern {
            pattern,
            constraints,
        } => format!("manifest:{}|{:?}", pattern.raw(), constraints.alternatives),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerSnapshotDecision {
    Created,
    SkippedUnchanged,
}

#[derive(Debug, Clone)]
pub struct SnapshotCreated {
    pub snapshots: GameSnapshots,
    pub local_archive_path: PathBuf,
    pub remote_archive_path: String,
}

pub struct CaptureSnapshotOptions<'a> {
    pub backup_base: &'a Path,
    pub preset: crate::backup::CompressionPreset,
    pub describe: &'a str,
    pub parent_date: Option<String>,
    pub created_by: CreatedBy,
    pub source_fingerprint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AutoBackupsCleanupResult {
    pub snapshots: GameSnapshots,
    pub deleted_remote_paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SnapshotDeleted {
    pub snapshots: GameSnapshots,
    pub remote_archive_path: String,
}

#[derive(Debug, Clone)]
pub struct BatchSnapshotsDeleted {
    pub snapshots: GameSnapshots,
    pub deleted_remote_paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GameDeleted {
    pub remote_game_dir_path: String,
}

impl GameDraft {
    /// Convert frontend/IPC draft to persisted game model with stable save-unit IDs.
    pub fn into_game(self, existing: Option<&Game>) -> Game {
        let mut existing_ids_by_identity: HashMap<String, VecDeque<u32>> = HashMap::new();
        let mut next_save_unit_id = existing.map(|game| game.next_save_unit_id).unwrap_or(0);

        if let Some(existing_game) = existing {
            for save_unit in &existing_game.save_paths {
                let identity = save_unit_identity(&save_unit.source);
                existing_ids_by_identity
                    .entry(identity)
                    .or_default()
                    .push_back(save_unit.id);
                if save_unit.id >= next_save_unit_id {
                    next_save_unit_id = save_unit.id.saturating_add(1);
                }
            }
        }

        let mut used_ids = HashSet::new();
        let mut save_paths = Vec::with_capacity(self.save_paths.len());
        for draft in self.save_paths {
            let identity = save_unit_identity(&draft.source);
            let id = if let Some(id) = draft.id {
                used_ids.insert(id);
                if id >= next_save_unit_id {
                    next_save_unit_id = id.saturating_add(1);
                }
                id
            } else {
                let reused_id = existing_ids_by_identity.get_mut(&identity).and_then(|ids| {
                    while let Some(candidate) = ids.pop_front() {
                        if !used_ids.contains(&candidate) {
                            return Some(candidate);
                        }
                    }
                    None
                });

                if let Some(id) = reused_id {
                    used_ids.insert(id);
                    if id >= next_save_unit_id {
                        next_save_unit_id = id.saturating_add(1);
                    }
                    id
                } else {
                    while used_ids.contains(&next_save_unit_id) {
                        next_save_unit_id = next_save_unit_id.saturating_add(1);
                    }
                    let id = next_save_unit_id;
                    used_ids.insert(id);
                    next_save_unit_id = next_save_unit_id.saturating_add(1);
                    id
                }
            };

            save_paths.push(SaveUnit {
                id,
                source: draft.source,
                delete_before_apply: draft.delete_before_apply,
                enabled: draft.enabled,
            });
        }

        let mut game = Game {
            name: self.name,
            storage_key: existing
                .and_then(|g| {
                    if g.storage_key.is_empty() {
                        None
                    } else {
                        Some(g.storage_key.clone())
                    }
                })
                .unwrap_or_default(),
            save_paths,
            game_paths: self.game_paths,
            next_save_unit_id,
            cloud_sync_enabled: existing.map(|game| game.cloud_sync_enabled).unwrap_or(true),
            auto_backup: existing.and_then(|game| game.auto_backup.clone()),
            ludusavi_meta: self
                .ludusavi_meta
                .or_else(|| existing.and_then(|g| g.ludusavi_meta.clone())),
            device_bindings: if self.device_bindings.is_empty() {
                existing
                    .map(|g| g.device_bindings.clone())
                    .unwrap_or_default()
            } else {
                self.device_bindings
            },
        };
        game.normalize_save_unit_ids();
        game
    }
}

impl Game {
    /// Build a `PathContext` from this game's metadata for path variable resolution.
    /// Pass the current `Device` to include explicit Device Resources.
    pub fn path_context(&self, device: Option<&crate::device::Device>) -> PathContext {
        let device_id = device.map(|d| &d.id);
        let binding = device_id.and_then(|id| self.device_bindings.get(id));
        let selected_account_ids = binding.and_then(|binding| binding.account_ids.as_ref());
        let steam_accounts = device
            .into_iter()
            .flat_map(|device| device.store_accounts())
            .filter_map(|resource| match &resource.kind {
                DeviceResourceKind::StoreAccount { store, user_id }
                    if *store == StoreKind::Steam
                        && selected_account_ids.is_none_or(|ids| ids.contains(&resource.id)) =>
                {
                    Some(user_id.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let store_user_id = (steam_accounts.len() == 1).then(|| steam_accounts[0].clone());
        let base = match &self.ludusavi_meta {
            Some(meta) => PathContext {
                install_dirs: meta.install_dirs.clone(),
                steam_id: meta
                    .store_game_id(StoreKind::Steam)
                    .and_then(|id| id.parse().ok()),
                install_dir_cache: None,
                game_roots: Vec::new(),
                store_user_id: None,
            },
            None => PathContext::default(),
        };
        PathContext {
            game_roots: device
                .into_iter()
                .flat_map(|device| device.game_roots())
                .filter(|resource| {
                    binding
                        .and_then(|binding| binding.root_ids.as_ref())
                        .is_none_or(|ids| ids.contains(&resource.id))
                })
                .filter_map(|resource| match &resource.kind {
                    DeviceResourceKind::GameRoot { path, .. } => Some(path.clone()),
                    _ => None,
                })
                .collect(),
            store_user_id,
            ..base
        }
    }

    /// Build a `PathContext` using the current device from config.
    /// Convenience wrapper around `path_context()` for runtime callers.
    fn path_context_current_device(&self, config: &crate::config::Config) -> PathContext {
        self.path_context(config.devices.get(get_current_device_id()))
    }

    /// The directory/path component used for local backup storage and remote
    /// cloud paths. Returns `storage_key` when populated; otherwise computes
    /// a sanitized key from the display name as a safe fallback (this path
    /// should only be hit for un-migrated legacy data).
    pub fn backup_dir_name(&self) -> std::borrow::Cow<'_, str> {
        if self.storage_key.is_empty() {
            std::borrow::Cow::Owned(super::storage_key::generate_storage_key(&self.name))
        } else {
            std::borrow::Cow::Borrowed(&self.storage_key)
        }
    }

    /// Return whether two persisted game references identify the same game.
    ///
    /// Current configs use `storage_key` as the stable identity. Legacy configs
    /// can still hold empty keys in secondary references, so those fall back to
    /// display-name matching.
    pub fn references_same_game(&self, other: &Game) -> bool {
        if !self.storage_key.is_empty() && !other.storage_key.is_empty() {
            return self.storage_key == other.storage_key;
        }

        self.name == other.name
    }

    /// Remote cloud path prefix for this game, e.g. `save_data/Game Name`.
    pub fn remote_path_prefix(&self) -> String {
        format!("save_data/{}", self.backup_dir_name())
    }

    /// Normalize save-unit IDs to keep them unique and stable inside this game.
    ///
    /// This is the backend safety net for all callers (frontend/CLI/FFI):
    /// - keep the first occurrence of each existing ID;
    /// - reassign only duplicated IDs using the monotonic counter;
    /// - move `next_save_unit_id` forward to one past the highest assigned ID.
    pub fn normalize_save_unit_ids(&mut self) {
        let mut used_ids = HashSet::with_capacity(self.save_paths.len());
        let mut next_id = self.next_save_unit_id;

        for save_unit in &self.save_paths {
            if used_ids.insert(save_unit.id) && save_unit.id >= next_id {
                next_id = save_unit.id.saturating_add(1);
            }
        }

        used_ids.clear();
        for save_unit in &mut self.save_paths {
            if used_ids.insert(save_unit.id) {
                continue;
            }
            while used_ids.contains(&next_id) {
                next_id = next_id.saturating_add(1);
            }
            save_unit.id = next_id;
            used_ids.insert(next_id);
            next_id = next_id.saturating_add(1);
        }

        self.next_save_unit_id = next_id;
    }

    pub fn get_game_snapshots_info(&self) -> Result<GameSnapshots, BackupError> {
        let backup_path = get_backup_path()?
            .join(self.backup_dir_name().as_ref())
            .join("Backups.json");
        let mut backup_info: GameSnapshots = serde_json::from_slice(&fs::read(backup_path)?)?;
        backup_info.normalize_heads();
        Ok(backup_info)
    }
    pub fn set_game_snapshots_info(&self, new_info: &GameSnapshots) -> Result<(), BackupError> {
        let saves_path = get_backup_path()?
            .join(self.backup_dir_name().as_ref())
            .join("Backups.json");
        // 处理文件夹不存在的情况，一般发生在初次下载云存档时
        let prefix_root = saves_path.parent().ok_or(BackupError::NonePathError)?;
        if !prefix_root.exists() {
            fs::create_dir_all(prefix_root)?;
        }
        let mut normalized = new_info.clone();
        normalized.normalize_heads();
        fs::write(saves_path, serde_json::to_string_pretty(&normalized)?)?;
        Ok(())
    }
    pub async fn create_snapshot(&self, describe: &str) -> Result<SnapshotCreated, BackupError> {
        self.create_snapshot_with_parent(describe, None, CreatedBy::Manual)
            .await
    }

    pub async fn create_snapshot_with_parent(
        &self,
        describe: &str,
        parent_date: Option<String>,
        created_by: CreatedBy,
    ) -> Result<SnapshotCreated, BackupError> {
        let backup_path = get_backup_path()?.join(self.backup_dir_name().as_ref());
        // Keep the timestamp format sortable so lexicographic order equals chronological order.
        let date = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let save_paths = &self.save_paths; // everything you should copy
        let config = get_config()?;
        let preset = config.settings.compression_preset;

        let zip_path = backup_path.join([&date, ".zip"].concat());
        // 获取压缩后的文件大小
        let path_ctx = self.path_context_current_device(&config);
        let file_size = match ZipBackend.compress(save_paths, &zip_path, preset, Some(&path_ctx)) {
            Ok(size) => size,
            Err(e) => {
                // delete the zip if failed to write
                fs::remove_file(&zip_path)?;
                return Err(BackupError::Compress(e));
            }
        };

        let mut infos = self.get_game_snapshots_info()?;

        let parent = parent_date.or_else(|| infos.current_device_head().cloned());

        let game_snapshots_info = Snapshot {
            date: date.clone(),
            describe: describe.to_string(),
            path: zip_path
                .to_str()
                .ok_or(BackupError::NonePathError)?
                .to_string(),
            archive_format: crate::backup::ArchiveFormat::Zip,
            size: file_size,
            parent,
            archive_hash: None,
            device_id: Some(get_current_device_id().clone()),
            created_by,
        };
        infos.backups.push(game_snapshots_info);

        infos.set_current_device_head(Some(date.clone()));

        self.set_game_snapshots_info(&infos)?;

        Ok(SnapshotCreated {
            snapshots: infos,
            remote_archive_path: format!("save_data/{}/{date}.zip", self.backup_dir_name()),
            local_archive_path: zip_path,
        })
    }

    /// Persist a snapshot from a fully preflighted immutable capture plan.
    /// Configuration and host discovery stay outside this domain operation.
    pub async fn create_snapshot_from_capture_plan(
        &self,
        plan: &CapturePlan,
        options: CaptureSnapshotOptions<'_>,
    ) -> Result<SnapshotCreated, BackupError> {
        let backup_path = options.backup_base.join(self.backup_dir_name().as_ref());
        fs::create_dir_all(&backup_path)?;
        let date = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let archive_format = ArchiveFormat::SevenZ;
        let archive_path = backup_path.join(archive_file_name(&date, archive_format));
        let file_size = SevenZBackend.compress_capture_plan(
            plan,
            &archive_path,
            options.preset,
            options.source_fingerprint,
        )?;

        let mut infos = self.get_game_snapshots_info()?;
        let parent = options
            .parent_date
            .or_else(|| infos.current_device_head().cloned());
        infos.backups.push(Snapshot {
            date: date.clone(),
            describe: options.describe.to_string(),
            path: archive_path.to_string_lossy().into_owned(),
            archive_format,
            size: file_size,
            parent,
            archive_hash: None,
            device_id: Some(get_current_device_id().clone()),
            created_by: options.created_by,
        });
        infos.set_current_device_head(Some(date.clone()));
        self.set_game_snapshots_info(&infos)?;

        Ok(SnapshotCreated {
            snapshots: infos,
            remote_archive_path: format!(
                "save_data/{}/{}",
                self.backup_dir_name(),
                archive_file_name(&date, archive_format)
            ),
            local_archive_path: archive_path,
        })
    }

    pub async fn create_timer_snapshot_if_changed(
        &self,
        describe: &str,
    ) -> Result<TimerSnapshotDecision, BackupError> {
        self.create_snapshot_if_changed(describe, CreatedBy::Timer)
            .await
    }

    pub async fn create_snapshot_if_changed(
        &self,
        describe: &str,
        created_by: CreatedBy,
    ) -> Result<TimerSnapshotDecision, BackupError> {
        let infos = self.get_game_snapshots_info()?;
        // `date` uses `%Y-%m-%d_%H-%M-%S`, so max lexicographic value is the newest snapshot.
        let latest_auto_snapshot = infos
            .backups
            .iter()
            .filter(|snapshot| snapshot.created_by.is_automatic_backup())
            .max_by_key(|snapshot| &snapshot.date);

        let Some(latest_auto_snapshot) = latest_auto_snapshot else {
            self.create_snapshot_with_parent(describe, None, created_by)
                .await?;
            return Ok(TimerSnapshotDecision::Created);
        };

        let config = get_config()?;
        let path_ctx = self.path_context_current_device(&config);
        let current_fingerprint = match fingerprint_source_state(&self.save_paths, Some(&path_ctx))
        {
            Ok(fingerprint) => fingerprint,
            Err(err) => {
                warn!(
                    target: "rgsm::backup::game",
                    "Failed to fingerprint current save state, fallback to creating snapshot: {err:?}"
                );
                self.create_snapshot_with_parent(describe, None, created_by)
                    .await?;
                return Ok(TimerSnapshotDecision::Created);
            }
        };

        let latest_zip_path = PathBuf::from(&latest_auto_snapshot.path);
        // Prefer stored fingerprint from ZIP comment (handles registry units),
        // fall back to scanning ZIP entries for legacy archives.
        let previous_fingerprint = read_stored_fingerprint(&latest_zip_path)
            .or_else(|| fingerprint_zip_state(&latest_zip_path).ok().flatten());

        match previous_fingerprint {
            Some(ref fp) if *fp == current_fingerprint => {
                info!(
                    target: "rgsm::backup::game",
                    "Skip timer auto backup for game {} because fingerprint is unchanged",
                    self.name
                );
                Ok(TimerSnapshotDecision::SkippedUnchanged)
            }
            Some(_) => {
                self.create_snapshot_with_parent(describe, None, created_by)
                    .await?;
                Ok(TimerSnapshotDecision::Created)
            }
            None => {
                info!(
                    target: "rgsm::backup::game",
                    "Latest automatic backup is legacy format; create one new automatic backup before dedup"
                );
                self.create_snapshot_with_parent(describe, None, created_by)
                    .await?;
                Ok(TimerSnapshotDecision::Created)
            }
        }
    }

    pub async fn cleanup_old_auto_backups(
        &self,
        max_count: u32,
    ) -> Result<AutoBackupsCleanupResult, BackupError> {
        if max_count == 0 {
            // 0 means unlimited, no cleanup needed
            return Ok(AutoBackupsCleanupResult {
                snapshots: self.get_game_snapshots_info()?,
                deleted_remote_paths: Vec::new(),
            });
        }

        let infos = self.get_game_snapshots_info()?;

        // Filter auto backups by created_by (not describe string)
        let mut auto_backups: Vec<_> = infos
            .backups
            .iter()
            .filter(|snapshot| snapshot.created_by.is_automatic_backup())
            .collect();

        // If we're within the limit, no cleanup needed
        if auto_backups.len() <= max_count as usize {
            return Ok(AutoBackupsCleanupResult {
                snapshots: infos,
                deleted_remote_paths: Vec::new(),
            });
        }

        // Sort by date (oldest first). Date string format preserves chronological order.
        auto_backups.sort_by(|a, b| a.date.cmp(&b.date));

        // Collect dates of oldest auto backups that exceed the limit
        let to_delete_count = auto_backups.len() - max_count as usize;
        let dates_to_delete: Vec<String> = auto_backups[..to_delete_count]
            .iter()
            .map(|snapshot| snapshot.date.clone())
            .collect();

        // Reuse batch_delete_snapshots for consistent parent-chain re-linking and HEAD update
        let result = self.batch_delete_snapshots(&dates_to_delete).await?;

        for date in &dates_to_delete {
            info!(target:"rgsm::backup::game", "Removed old auto backup: {date}");
        }

        Ok(AutoBackupsCleanupResult {
            snapshots: result.snapshots,
            deleted_remote_paths: result.deleted_remote_paths,
        })
    }
    /// Decompress a snapshot archive and update HEAD.
    ///
    /// **Pre-restore checks** (extra backup, integrity verification) are
    /// handled by the hook pipeline's `fire_before_restore` gate — callers
    /// must invoke that *before* calling this method.
    pub fn restore_snapshot(
        &self,
        date: &str,
        notifier: Option<&dyn RestoreNotifier>,
    ) -> Result<GameSnapshots, BackupError> {
        let config = get_config()?;
        let path_ctx = self.path_context_current_device(&config);
        self.restore_snapshot_with_context(
            date,
            notifier,
            &crate::config::resolve_backup_path(&config.backup_path),
            &path_ctx,
        )
    }

    pub fn restore_snapshot_with_context(
        &self,
        date: &str,
        notifier: Option<&dyn RestoreNotifier>,
        backup_base: &Path,
        path_ctx: &PathContext,
    ) -> Result<GameSnapshots, BackupError> {
        let archive_path = backup_base
            .join(self.backup_dir_name().as_ref())
            .join(format!("{date}.zip"));
        ZipBackend.decompress(&self.save_paths, &archive_path, notifier, Some(path_ctx))?;

        let mut infos = self.get_game_snapshots_info()?;
        infos.set_current_device_head(Some(date.to_string()));
        self.set_game_snapshots_info(&infos)?;

        Ok(infos)
    }
    pub fn create_overwrite_snapshot(
        &self,
        max_extra_backup_count: u32,
    ) -> Result<(), BackupError> {
        let extra_backup_path = get_backup_path()?
            .join(self.backup_dir_name().as_ref())
            .join("extra_backup");

        // Create extra backup
        if !extra_backup_path.exists() {
            fs::create_dir_all(&extra_backup_path)?;
        }
        let date = chrono::Local::now()
            .format("Overwrite_%Y-%m-%d_%H-%M-%S")
            .to_string();
        let zip_path = &extra_backup_path.join([&date, ".zip"].concat());
        let config = get_config()?;
        let preset = config.settings.compression_preset;
        let path_ctx = self.path_context_current_device(&config);
        if let Err(e) = ZipBackend.compress(&self.save_paths, zip_path, preset, Some(&path_ctx)) {
            if let Err(rm_err) = fs::remove_file(zip_path) {
                warn!(
                    target: "rgsm::backup",
                    "Failed to cleanup failed extra backup zip: {:?}",
                    rm_err
                );
            }
            return Err(e.into());
        }

        cleanup_oldest_extra_backups(&extra_backup_path, max_extra_backup_count)?;
        Result::Ok(())
    }

    pub fn create_overwrite_snapshot_from_capture_plan(
        &self,
        plan: &CapturePlan,
        backup_base: &Path,
        preset: crate::backup::CompressionPreset,
        max_extra_backup_count: u32,
    ) -> Result<(), BackupError> {
        let extra_backup_path = backup_base
            .join(self.backup_dir_name().as_ref())
            .join("extra_backup");
        fs::create_dir_all(&extra_backup_path)?;
        let date = chrono::Local::now()
            .format("Overwrite_%Y-%m-%d_%H-%M-%S")
            .to_string();
        let archive_path = extra_backup_path.join(archive_file_name(&date, ArchiveFormat::SevenZ));
        SevenZBackend.compress_capture_plan(plan, &archive_path, preset, None)?;
        cleanup_oldest_extra_backups(&extra_backup_path, max_extra_backup_count)?;
        Ok(())
    }
    pub async fn delete_snapshot(&self, date: &str) -> Result<SnapshotDeleted, BackupError> {
        let mut saves = self.get_game_snapshots_info()?;
        let deleted = saves
            .backups
            .iter()
            .find(|snapshot| snapshot.date == date)
            .ok_or_else(|| {
                BackupError::Unexpected(anyhow::anyhow!("Snapshot '{date}' was not found"))
            })?;
        let backup_dir = get_backup_path()?.join(self.backup_dir_name().as_ref());
        let archive_path = crate::backup::snapshot_archive_path(&backup_dir, deleted);
        fs::remove_file(&archive_path)?;
        let remote_archive_path = crate::backup::remote_archive_path(
            self.backup_dir_name().as_ref(),
            date,
            deleted.archive_format,
        )
        .to_string_lossy()
        .replace('\\', "/");

        // Find the parent of the deleted node
        let deleted_parent = saves
            .backups
            .iter()
            .find(|x| x.date == date)
            .and_then(|x| x.parent.clone());

        // Find children of the deleted node BEFORE re-parenting them
        let children_dates: Vec<String> = saves
            .backups
            .iter()
            .filter(|x| x.parent.as_deref() == Some(date))
            .map(|x| x.date.clone())
            .collect();

        // Update children's parent to point to deleted node's parent
        for snapshot in saves.backups.iter_mut() {
            if snapshot.parent.as_deref() == Some(date) {
                snapshot.parent = deleted_parent.clone();
            }
        }

        let replacement_head = if !children_dates.is_empty() {
            children_dates.iter().max().cloned()
        } else if deleted_parent.is_some() {
            deleted_parent.clone()
        } else {
            saves
                .backups
                .iter()
                .filter(|x| x.date != date)
                .max_by_key(|x| &x.date)
                .map(|x| x.date.clone())
        };
        let affected_devices: Vec<_> = saves
            .head_entries()
            .filter(|(_, head)| head.as_str() == date)
            .map(|(device_id, _)| device_id.clone())
            .collect();
        for device_id in affected_devices {
            saves.set_head_for_device(device_id, replacement_head.clone());
        }

        saves.backups.retain(|x| x.date != date);
        self.set_game_snapshots_info(&saves)?;

        Ok(SnapshotDeleted {
            snapshots: saves,
            remote_archive_path,
        })
    }

    pub async fn batch_delete_snapshots(
        &self,
        dates: &[String],
    ) -> Result<BatchSnapshotsDeleted, BackupError> {
        if dates.is_empty() {
            return Ok(BatchSnapshotsDeleted {
                snapshots: self.get_game_snapshots_info()?,
                deleted_remote_paths: Vec::new(),
            });
        }

        let backup_dir = get_backup_path()?.join(self.backup_dir_name().as_ref());
        let mut saves = self.get_game_snapshots_info()?;
        let to_delete: HashSet<&str> = dates.iter().map(|d| d.as_str()).collect();

        // Resolve every archive from persisted metadata before mutating the snapshot graph.
        let mut deleted_remote_paths = Vec::new();
        for date in dates {
            let snapshot = saves
                .backups
                .iter()
                .find(|snapshot| snapshot.date == *date)
                .ok_or_else(|| {
                    BackupError::Unexpected(anyhow::anyhow!("Snapshot '{date}' was not found"))
                })?;
            let archive_path = crate::backup::snapshot_archive_path(&backup_dir, snapshot);
            if archive_path.exists() {
                fs::remove_file(&archive_path)?;
            }
            deleted_remote_paths.push(
                crate::backup::remote_archive_path(
                    self.backup_dir_name().as_ref(),
                    date,
                    snapshot.archive_format,
                )
                .to_string_lossy()
                .replace('\\', "/"),
            );
        }

        // Build parent lookup for resolving ancestor chains
        let parent_map: HashMap<String, Option<String>> = saves
            .backups
            .iter()
            .map(|s| (s.date.clone(), s.parent.clone()))
            .collect();

        // Find the nearest surviving ancestor for a deleted date
        let find_surviving_ancestor = |date: &str| -> Option<String> {
            let mut current = parent_map.get(date).cloned().flatten();
            while let Some(p) = current {
                if !to_delete.contains(p.as_str()) {
                    return Some(p);
                }
                current = parent_map.get(&p).cloned().flatten();
            }
            None
        };

        // Pre-compute surviving ancestor for each deleted date
        let surviving_ancestors: HashMap<&str, Option<String>> = to_delete
            .iter()
            .map(|&d| (d, find_surviving_ancestor(d)))
            .collect();

        // Re-parent surviving children of deleted nodes
        for snapshot in saves.backups.iter_mut() {
            if to_delete.contains(snapshot.date.as_str()) {
                continue;
            }
            if let Some(ref parent) = snapshot.parent
                && to_delete.contains(parent.as_str())
            {
                snapshot.parent = surviving_ancestors[parent.as_str()].clone();
            }
        }

        let newest_surviving = saves
            .backups
            .iter()
            .filter(|s| !to_delete.contains(s.date.as_str()))
            .max_by_key(|s| &s.date)
            .map(|s| s.date.clone());
        let affected_devices: Vec<_> = saves
            .head_entries()
            .filter(|(_, head)| to_delete.contains(head.as_str()))
            .map(|(device_id, head)| {
                let replacement =
                    find_surviving_ancestor(head).or_else(|| newest_surviving.clone());
                (device_id.clone(), replacement)
            })
            .collect();
        for (device_id, replacement) in affected_devices {
            saves.set_head_for_device(device_id, replacement);
        }

        saves
            .backups
            .retain(|s| !to_delete.contains(s.date.as_str()));
        self.set_game_snapshots_info(&saves)?;

        Ok(BatchSnapshotsDeleted {
            snapshots: saves,
            deleted_remote_paths,
        })
    }

    pub async fn delete_game(&self) -> Result<GameDeleted, BackupError> {
        let mut config = get_config()?;
        let backup_path = get_backup_path()?.join(self.backup_dir_name().as_ref());
        fs::remove_dir_all(&backup_path)?;

        config
            .games
            .retain(|x| x.backup_dir_name() != self.backup_dir_name());
        // TODO(config-hooks): move this cleanup into a gated config-change hook
        // once config commits can mutate/abort the pending write transaction.
        config.remove_deleted_game_references(self);
        set_config_local(&config)?;

        info!(target:"rgsm::backup::game",
            "Delete Game(local only): {:#?}",
            backup_path.to_str().ok_or(BackupError::NonePathError)?
        );

        Ok(GameDeleted {
            remote_game_dir_path: self.remote_path_prefix(),
        })
    }
    pub async fn set_snapshot_description(
        &self,
        date: &str,
        describe: &str,
    ) -> Result<GameSnapshots, BackupError> {
        let mut saves = self.get_game_snapshots_info()?;
        let pos = saves.backups.iter().position(|x| x.date == date).ok_or(
            BackupError::BackupNotExist {
                name: self.name.clone(),
                date: date.to_string(),
            },
        )?;
        saves.backups[pos].describe = describe.to_string();
        self.set_game_snapshots_info(&saves)?;
        Ok(saves)
    }
}
