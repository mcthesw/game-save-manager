use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;

use crate::default_value;
use crate::device::DeviceId;

/// A save unit should be a file or a folder
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub enum SaveUnitType {
    File,
    Folder,
}

/// A save unit declares one of the files/folders
/// that should be backup for a game
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct SaveUnit {
    pub unit_type: SaveUnitType,
    #[serde(default)] // 如果反序列化时字段不存在，则使用默认值 (空 HashMap)
    pub paths: HashMap<DeviceId, String>, // 存储不同设备的路径
    #[serde(default = "default_value::default_false")]
    pub delete_before_apply: bool,
}

impl SaveUnit {
    /// 获取指定设备的路径
    pub fn get_path_for_device(&self, device_id: &DeviceId) -> Option<&String> {
        self.paths.get(device_id)
    }
}
