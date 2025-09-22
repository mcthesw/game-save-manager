use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::backup::Game;
use crate::cloud_sync::CloudSettings;
use crate::config::{QuickActionsSettings, SaveListExpandBehavior, Settings};
use crate::default_value;
use crate::device::{Device, DeviceId};
use crate::preclude::*;

/// The software's configuration
/// include the version, backup's location path, games'info,
/// and the settings
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct Config {
    pub version: String,
    pub backup_path: String,
    pub games: Vec<Game>,
    pub settings: Settings,
    #[serde(default = "default_value::empty_vec")]
    pub favorites: Vec<FavoriteTreeNode>,
    #[serde(default = "default_value::default")]
    pub quick_action: QuickActionsSettings,
    /// 设备ID到设备名称的映射
    #[serde(default = "default_value::empty_map")]
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

impl Default for Config {
    fn default() -> Self {
        Config {
            version: String::from(std::env!("CARGO_PKG_VERSION")),
            backup_path: String::from("./save_data"),
            games: Vec::new(),
            settings: Settings {
                prompt_when_not_described: false,
                extra_backup_when_apply: true,
                show_edit_button: false,
                prompt_when_auto_backup: true,
                cloud_settings: CloudSettings::default(),
                exit_to_tray: true,
                locale: default_value::default_locale(),
                default_delete_before_apply: false,
                default_expend_favorites_tree: false,
                home_page: default_value::default_home_page(),
                log_to_file: true,
                add_new_to_favorites: false,
                save_list_expand_behavior: SaveListExpandBehavior::default(),
                save_list_last_expanded: false,
            },
            favorites: vec![],
            quick_action: QuickActionsSettings::default(),
            devices: HashMap::new(),
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
