use log::{info, warn};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

use crate::backup::state_fingerprint::{fingerprint_source_state, fingerprint_zip_state};
use crate::backup::{
    GameSnapshots, SaveUnit, Snapshot, TIMER_AUTO_BACKUP_DESCRIPTION, compress_to_file,
    decompress_from_file,
};
use crate::config::{get_backup_path, get_config, set_config_local};
use crate::device::DeviceId;
use crate::ipc_handler::{IpcNotification, NotificationLevel};
use crate::preclude::*;

/// A game struct contains the save units and the game's launcher
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct Game {
    pub name: String,
    pub save_paths: Vec<SaveUnit>,
    // 使用 HashMap 存储不同设备的启动路径
    // Key: DeviceId (String), Value: Path (String)
    #[serde(default)]
    pub game_paths: HashMap<DeviceId, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimerSnapshotDecision {
    Created,
    SkippedUnchanged,
}

#[derive(Debug, Clone)]
pub struct SnapshotCreated {
    pub snapshots: GameSnapshots,
    pub local_zip_path: PathBuf,
    pub remote_zip_path: String,
}

#[derive(Debug, Clone)]
pub struct AutoBackupsCleanupResult {
    pub snapshots: GameSnapshots,
    pub deleted_remote_paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SnapshotDeleted {
    pub snapshots: GameSnapshots,
    pub remote_zip_path: String,
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

impl Game {
    pub fn get_game_snapshots_info(&self) -> Result<GameSnapshots, BackupError> {
        let backup_path = get_backup_path()?.join(&self.name).join("Backups.json");
        let backup_info = serde_json::from_slice(&fs::read(backup_path)?)?;
        Ok(backup_info)
    }
    pub fn set_game_snapshots_info(&self, new_info: &GameSnapshots) -> Result<(), BackupError> {
        let saves_path = get_backup_path()?.join(&self.name).join("Backups.json");
        // 处理文件夹不存在的情况，一般发生在初次下载云存档时
        let prefix_root = saves_path.parent().ok_or(BackupError::NonePathError)?;
        if !prefix_root.exists() {
            fs::create_dir_all(prefix_root)?;
        }
        fs::write(saves_path, serde_json::to_string_pretty(&new_info)?)?;
        Ok(())
    }
    pub async fn create_snapshot(&self, describe: &str) -> Result<SnapshotCreated, BackupError> {
        let backup_path = get_backup_path()?.join(&self.name); // the backup zip file should be placed here
        // Keep the timestamp format sortable so lexicographic order equals chronological order.
        let date = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let save_paths = &self.save_paths; // everything you should copy

        let zip_path = backup_path.join([&date, ".zip"].concat());
        // 获取压缩后的文件大小
        let file_size = match compress_to_file(save_paths, &zip_path) {
            Ok(size) => size,
            Err(e) => {
                // delete the zip if failed to write
                fs::remove_file(&zip_path)?;
                return Err(BackupError::Compress(e));
            }
        };

        let mut infos = self.get_game_snapshots_info()?;

        // Set parent based on current HEAD
        let parent = infos.head.clone();

        let game_snapshots_info = Snapshot {
            date: date.clone(),
            describe: describe.to_string(),
            path: zip_path
                .to_str()
                .ok_or(BackupError::NonePathError)?
                .to_string(),
            size: file_size,
            parent,
        };
        infos.backups.push(game_snapshots_info);

        // Update HEAD to point to the new snapshot
        infos.head = Some(date.clone());

        self.set_game_snapshots_info(&infos)?;

        Ok(SnapshotCreated {
            snapshots: infos,
            remote_zip_path: format!("save_data/{}/{date}.zip", self.name),
            local_zip_path: zip_path,
        })
    }

    pub async fn create_timer_snapshot_if_changed(
        &self,
        describe: &str,
    ) -> Result<TimerSnapshotDecision, BackupError> {
        let infos = self.get_game_snapshots_info()?;
        // `date` uses `%Y-%m-%d_%H-%M-%S`, so max lexicographic value is the newest snapshot.
        let latest_auto_snapshot = infos
            .backups
            .iter()
            .filter(|snapshot| snapshot.describe == TIMER_AUTO_BACKUP_DESCRIPTION)
            .max_by_key(|snapshot| &snapshot.date);

        let Some(latest_auto_snapshot) = latest_auto_snapshot else {
            self.create_snapshot(describe).await?;
            return Ok(TimerSnapshotDecision::Created);
        };

        let current_fingerprint = match fingerprint_source_state(&self.save_paths) {
            Ok(fingerprint) => fingerprint,
            Err(err) => {
                warn!(
                    target: "rgsm::backup::game",
                    "Failed to fingerprint current save state, fallback to creating snapshot: {err:?}"
                );
                self.create_snapshot(describe).await?;
                return Ok(TimerSnapshotDecision::Created);
            }
        };

        let latest_zip_path = PathBuf::from(&latest_auto_snapshot.path);
        match fingerprint_zip_state(&latest_zip_path) {
            Ok(Some(previous_fingerprint)) if previous_fingerprint == current_fingerprint => {
                info!(
                    target: "rgsm::backup::game",
                    "Skip timer auto backup for game {} because fingerprint is unchanged",
                    self.name
                );
                Ok(TimerSnapshotDecision::SkippedUnchanged)
            }
            Ok(Some(_)) => {
                self.create_snapshot(describe).await?;
                Ok(TimerSnapshotDecision::Created)
            }
            Ok(None) => {
                info!(
                    target: "rgsm::backup::game",
                    "Latest timer backup is legacy format; create one new timer backup before dedup"
                );
                self.create_snapshot(describe).await?;
                Ok(TimerSnapshotDecision::Created)
            }
            Err(err) => {
                warn!(
                    target: "rgsm::backup::game",
                    "Failed to fingerprint previous timer backup, fallback to creating snapshot: {err:?}"
                );
                self.create_snapshot(describe).await?;
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

        // Filter auto backups (Timer backups only)
        let mut auto_backups: Vec<_> = infos
            .backups
            .iter()
            .filter(|snapshot| snapshot.describe == TIMER_AUTO_BACKUP_DESCRIPTION)
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
    pub fn restore_snapshot(
        &self,
        date: &str,
        app_handle: Option<&AppHandle>,
    ) -> Result<GameSnapshots, BackupError> {
        let config = get_config()?;
        let backup_path = get_backup_path()?.join(&self.name);
        if config.settings.extra_backup_when_apply {
            info!(target:"rgsm::backup::game","Creating extra backup.");
            if let Err(e) = self.create_overwrite_snapshot(config.settings.max_extra_backup_count) {
                if let Some(app_handle) = app_handle {
                    app_handle
                        .emit(
                            "Notification",
                            IpcNotification {
                                level: NotificationLevel::warning,
                                title: "WARNING".to_string(),
                                msg: t!("backend.backup.extra_backup_file_not_exist").to_string(),
                            },
                        )
                        .map_err(anyhow::Error::from)?;
                }
                warn!(target:"rgsm::backup::game","Failed to create extra backup: {:?}", e);
            }
        }
        decompress_from_file(&self.save_paths, &backup_path, date, app_handle)?;

        // Update HEAD to point to the restored snapshot
        let mut infos = self.get_game_snapshots_info()?;
        infos.head = Some(date.to_string());
        self.set_game_snapshots_info(&infos)?;

        Ok(infos)
    }
    pub fn create_overwrite_snapshot(
        &self,
        max_extra_backup_count: u32,
    ) -> Result<(), BackupError> {
        let extra_backup_path = get_backup_path()?.join(&self.name).join("extra_backup");

        // Create extra backup
        if !extra_backup_path.exists() {
            fs::create_dir_all(&extra_backup_path)?;
        }
        let date = chrono::Local::now()
            .format("Overwrite_%Y-%m-%d_%H-%M-%S")
            .to_string();
        let zip_path = &extra_backup_path.join([&date, ".zip"].concat());
        if let Err(e) = compress_to_file(&self.save_paths, zip_path) {
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
    pub async fn delete_snapshot(&self, date: &str) -> Result<SnapshotDeleted, BackupError> {
        let save_path = get_backup_path()?
            .join(&self.name)
            .join(date.to_string() + ".zip");
        fs::remove_file(&save_path)?;

        let mut saves = self.get_game_snapshots_info()?;

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

        // Update HEAD if it pointed to the deleted snapshot
        if saves.head.as_deref() == Some(date) {
            saves.head = if !children_dates.is_empty() {
                // Set HEAD to the newest child (latest date)
                children_dates.iter().max().cloned()
            } else if deleted_parent.is_some() {
                // No children, fall back to parent
                deleted_parent.clone()
            } else {
                // Deleted node was a root with no children
                // Find the newest remaining snapshot
                saves
                    .backups
                    .iter()
                    .filter(|x| x.date != date)
                    .max_by_key(|x| &x.date)
                    .map(|x| x.date.clone())
            };
        }

        saves.backups.retain(|x| x.date != date);
        self.set_game_snapshots_info(&saves)?;

        Ok(SnapshotDeleted {
            snapshots: saves,
            remote_zip_path: format!("save_data/{}/{date}.zip", self.name),
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

        let backup_dir = get_backup_path()?.join(&self.name);
        let to_delete: HashSet<&str> = dates.iter().map(|d| d.as_str()).collect();

        // Delete zip files
        let mut deleted_remote_paths = Vec::new();
        for date in dates {
            let zip_path = backup_dir.join(format!("{date}.zip"));
            if zip_path.exists() {
                fs::remove_file(&zip_path)?;
            }
            deleted_remote_paths.push(format!("save_data/{}/{date}.zip", self.name));
        }

        let mut saves = self.get_game_snapshots_info()?;

        // Build parent lookup for resolving ancestor chains
        let parent_map: HashMap<&str, Option<&str>> = saves
            .backups
            .iter()
            .map(|s| (s.date.as_str(), s.parent.as_deref()))
            .collect();

        // Find the nearest surviving ancestor for a deleted date
        let find_surviving_ancestor = |date: &str| -> Option<String> {
            let mut current = parent_map.get(date).copied().flatten();
            while let Some(p) = current {
                if !to_delete.contains(p) {
                    return Some(p.to_string());
                }
                current = parent_map.get(p).copied().flatten();
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
            if let Some(ref parent) = snapshot.parent {
                if to_delete.contains(parent.as_str()) {
                    snapshot.parent = surviving_ancestors[parent.as_str()].clone();
                }
            }
        }

        // Update HEAD if it points to a deleted snapshot
        if let Some(ref head) = saves.head {
            if to_delete.contains(head.as_str()) {
                saves.head = saves
                    .backups
                    .iter()
                    .filter(|s| !to_delete.contains(s.date.as_str()))
                    .max_by_key(|s| &s.date)
                    .map(|s| s.date.clone());
            }
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
        let backup_path = get_backup_path()?.join(&self.name);
        fs::remove_dir_all(&backup_path)?;

        config.games.retain(|x| x.name != self.name);
        set_config_local(&config)?;

        info!(target:"rgsm::backup::game",
            "Delete Game(local only): {:#?}",
            backup_path.to_str().ok_or(BackupError::NonePathError)?
        );

        Ok(GameDeleted {
            remote_game_dir_path: format!("save_data/{}", self.name),
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

fn cleanup_oldest_extra_backups(
    extra_backup_path: &Path,
    max_count: u32,
) -> Result<(), BackupError> {
    if max_count == 0 {
        return Ok(());
    }

    let mut items: Vec<_> = extra_backup_path
        .read_dir()?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("zip") {
                return None;
            }
            let file_name = path.file_name()?.to_os_string();
            let modified = fs::metadata(&path).ok()?.modified().ok()?;
            Some((modified, file_name))
        })
        .collect();

    if items.len() <= max_count as usize {
        return Ok(());
    }

    items.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let to_delete = items.len() - max_count as usize;
    for (_mtime, file_name) in items.into_iter().take(to_delete) {
        let p = extra_backup_path.join(file_name);
        let _ = fs::remove_file(p);
    }

    Ok(())
}
