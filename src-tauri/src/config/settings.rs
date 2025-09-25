use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use specta::Type;

use crate::cloud_sync::CloudSettings;
use crate::preclude::*;

/// Settings that can be configured by user
#[derive(Debug, Serialize, Deserialize, Clone, Type, SmartDefault)]
#[serde(rename_all = "snake_case")]
pub enum SaveListExpandBehavior {
    AlwaysOpen,
    #[default]
    AlwaysClosed,
    RememberLast,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, SmartDefault)]
#[serde(default)]
pub struct Settings {
    #[default = true]
    pub prompt_when_not_described: bool,
    #[default = true]
    pub extra_backup_when_apply: bool,
    pub show_edit_button: bool,
    #[default = true]
    pub prompt_when_auto_backup: bool,
    #[default = true]
    pub exit_to_tray: bool,
    #[default]
    pub cloud_settings: CloudSettings,
    #[default = "\"zh_SIMPLIFIED\".into()"]
    pub locale: String,
    pub default_delete_before_apply: bool,
    pub default_expend_favorites_tree: bool,
    #[default = "\"/\".into()"]
    pub home_page: String,
    #[default = true]
    pub log_to_file: bool,
    pub add_new_to_favorites: bool,
    #[default]
    pub save_list_expand_behavior: SaveListExpandBehavior,
    pub save_list_last_expanded: bool,
}

impl Sanitizable for Settings {
    fn sanitize(self) -> Self {
        Settings {
            cloud_settings: self.cloud_settings.sanitize(),
            ..self
        }
    }
}
