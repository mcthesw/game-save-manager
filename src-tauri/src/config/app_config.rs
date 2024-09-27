use serde::{Deserialize, Serialize};

use crate::backup::Game;
use crate::cloud_sync::CloudSettings;
use crate::default_value;
use crate::traits::Sanitizable;

use super::{QuickActionsSettings, Settings};

/// The software's configuration
/// include the version, backup's location path, games'info,
/// and the settings
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub version: String,
    pub backup_path: String,
    pub games: Vec<Game>,
    pub settings: Settings,
    #[serde(default = "default_value::empty_vec")]
    pub favorites: Vec<FavoriteTreeNode>,
    pub quick_action: QuickActionsSettings,
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
            version: String::from("1.3.2"),
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
            },
            favorites: vec![],
            quick_action: QuickActionsSettings::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FavoriteTreeNode {
    node_id: String,
    label: String,
    is_leaf: bool,
    children: Option<Vec<Self>>,
}
