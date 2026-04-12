use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::default_value;
use crate::device::{DeviceId, get_current_device_id};
use crate::path_resolver::PathContext;
use crate::preclude::BackupFileError;

/// The kind of data a save unit backs up.
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub enum SaveUnitType {
    File,
    Folder,
    /// Windows Registry key tree (stored as `registry.json` inside the archive).
    WinRegistry,
}

/// A save unit declares one of the files/folders
/// that should be backup for a game.
///
/// The `id` field is a stable identifier used as the index prefix in V2 archives.
/// Unlike positional indices, it does not change when save units are added or removed,
/// ensuring old archives can always be restored correctly. The backend will
/// normalize duplicated IDs when persisting config.
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct SaveUnit {
    /// Stable identifier for this save unit, used as archive entry prefix in V2 format.
    /// Provided by the caller (frontend/CLI/FFI) and kept unique by backend normalization.
    #[serde(default)]
    pub id: u32,
    pub unit_type: SaveUnitType,
    #[serde(default)] // 如果反序列化时字段不存在，则使用默认值 (空 HashMap)
    pub paths: HashMap<DeviceId, String>, // 存储不同设备的路径
    #[serde(default = "default_value::default_false")]
    pub delete_before_apply: bool,
    #[serde(default = "default_value::default_true")]
    pub enabled: bool,
}

/// Frontend/IPC input shape for save-unit editing.
/// Existing rows may provide `id` to preserve archive compatibility during edits;
/// backend logic allocates IDs for new rows and normalizes duplicates.
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct SaveUnitDraft {
    #[serde(default)]
    pub id: Option<u32>,
    pub unit_type: SaveUnitType,
    #[serde(default)]
    pub paths: HashMap<DeviceId, String>,
    #[serde(default = "default_value::default_false")]
    pub delete_before_apply: bool,
    #[serde(default = "default_value::default_true")]
    pub enabled: bool,
}

impl SaveUnit {
    /// 获取指定设备的路径
    pub fn get_path_for_device(&self, device_id: &DeviceId) -> Option<&String> {
        self.paths.get(device_id)
    }

    /// Resolve this save unit path for the current device.
    pub fn resolve_path_for_current_device(
        &self,
        path_ctx: Option<&PathContext>,
    ) -> Result<PathBuf, BackupFileError> {
        let current_device_id = get_current_device_id();
        let unit_path_str = self
            .get_path_for_device(current_device_id)
            .ok_or(BackupFileError::NonePathError)?;
        let config =
            crate::config::get_config().map_err(|e| BackupFileError::Unexpected(e.into()))?;
        Ok(crate::path_resolver::resolve_path(
            unit_path_str,
            path_ctx,
            &config,
        )?)
    }
}
