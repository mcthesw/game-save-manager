use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use specta::Type;

use crate::backup::Game;
use crate::config::{QuickActionsSettings, Settings};
use crate::device::{Device, DeviceId};
use crate::preclude::*;

/// The software's configuration
/// include the version, backup's location path, games'info,
/// and the settings
#[derive(Debug, Serialize, Deserialize, Clone, Type, SmartDefault)]
#[serde(default)]
pub struct Config {
    #[default = "String::from(std::env!(\"CARGO_PKG_VERSION\"))"]
    pub version: String,
    #[default = "\"./save_data\".to_owned()"]
    pub backup_path: String,
    #[default]
    pub games: Vec<Game>,
    #[default = "Settings { prompt_when_not_described: false, ..Settings::default() }"]
    pub settings: Settings,
    #[default]
    pub favorites: Vec<FavoriteTreeNode>,
    #[default]
    pub quick_action: QuickActionsSettings,
    /// 设备ID到设备名称的映射
    #[default]
    pub devices: HashMap<DeviceId, Device>,
}

impl Sanitizable for Config {
    fn sanitize(self) -> Self {
        Config {
            settings: self.settings.sanitize(),
            ..self
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct FavoriteTreeNode {
    node_id: String,
    label: String,
    is_leaf: bool,
    children: Option<Vec<Self>>,
}
