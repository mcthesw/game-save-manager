use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::backup::{CompressionPreset, Game};
use crate::cloud_sync::CloudSettings;
use crate::config::{
    AppearanceSettings, QuickActionsSettings, SaveListExpandBehavior, SaveListSortMode, Settings,
    SortDirection,
};
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
            backup_path: String::from("save_data"),
            games: Vec::new(),
            settings: Settings {
                prompt_when_not_described: false,
                extra_backup_when_apply: true,
                confirm_before_apply_latest: true,
                confirm_before_apply_snapshot: true,
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
                vn_scan_dirs: default_value::empty_vec(),
                save_list_expand_behavior: SaveListExpandBehavior::default(),
                save_list_last_expanded: false,
                save_list_sort_mode: SaveListSortMode::default(),
                save_list_sort_direction: SortDirection::default(),
                max_auto_backup_count: 0,
                max_extra_backup_count: 5,
                appearance: AppearanceSettings::default(),
                compression_preset: CompressionPreset::default(),
                compute_archive_hash: false,
                verify_archive_before_apply: false,
            },
            favorites: vec![],
            quick_action: QuickActionsSettings::default(),
            devices: HashMap::new(),
        }
    }
}

impl Config {
    pub fn selected_quick_action_game(&self) -> Option<&Game> {
        self.quick_action.selected_game(&self.games)
    }

    pub fn remove_deleted_game_references(&mut self, deleted_game: &Game) -> bool {
        let quick_action_changed = self
            .quick_action
            .remove_deleted_game_reference(deleted_game);
        let favorites_changed =
            FavoriteTreeNode::remove_deleted_game_leaves(&mut self.favorites, deleted_game);

        quick_action_changed || favorites_changed
    }

    /// Locate a game by its stable identity, accepting legacy display-name
    /// callers while preferring `storage_key` when available.
    pub fn position_game_by_identity(&self, identity: &str) -> Option<usize> {
        self.games
            .iter()
            .position(|game| !identity.is_empty() && game.storage_key == identity)
            .or_else(|| self.games.iter().position(|game| game.name == identity))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type)]
pub struct FavoriteTreeNode {
    node_id: String,
    label: String,
    is_leaf: bool,
    children: Option<Vec<Self>>,
}

impl FavoriteTreeNode {
    fn remove_deleted_game_leaves(nodes: &mut Vec<Self>, deleted_game: &Game) -> bool {
        Self::remove_game_leaves(nodes, &deleted_game.name)
    }

    pub(crate) fn remove_game_leaves(nodes: &mut Vec<Self>, game_name: &str) -> bool {
        let mut changed = false;

        nodes.retain_mut(|node| {
            if node.is_leaf && node.label == game_name {
                changed = true;
                return false;
            }

            if let Some(children) = &mut node.children
                && Self::remove_game_leaves(children, game_name)
            {
                changed = true;
            }

            true
        });

        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_game(name: &str, storage_key: &str) -> Game {
        Game {
            name: name.to_string(),
            storage_key: storage_key.to_string(),
            save_paths: Vec::new(),
            game_paths: HashMap::new(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            ludusavi_meta: None,
            device_bindings: HashMap::new(),
        }
    }

    fn favorite_leaf(label: &str) -> FavoriteTreeNode {
        FavoriteTreeNode {
            node_id: format!("leaf-{label}"),
            label: label.to_string(),
            is_leaf: true,
            children: None,
        }
    }

    fn favorite_folder(label: &str, children: Vec<FavoriteTreeNode>) -> FavoriteTreeNode {
        FavoriteTreeNode {
            node_id: format!("folder-{label}"),
            label: label.to_string(),
            is_leaf: false,
            children: Some(children),
        }
    }

    #[test]
    fn cleanup_deleted_game_references_removes_matching_favorite_leaves() {
        let deleted_game = test_game("Deleted Game", "deleted-game-key");
        let mut config = Config {
            favorites: vec![
                favorite_leaf("Deleted Game"),
                favorite_folder(
                    "Folder",
                    vec![
                        favorite_leaf("Deleted Game"),
                        favorite_leaf("Remaining Game"),
                    ],
                ),
                favorite_folder("Deleted Game", vec![]),
            ],
            ..Config::default()
        };

        assert!(config.remove_deleted_game_references(&deleted_game));

        assert_eq!(config.favorites.len(), 2);
        assert_eq!(config.favorites[0].label, "Folder");
        assert_eq!(
            config.favorites[0]
                .children
                .as_ref()
                .expect("folder children should remain")
                .iter()
                .map(|node| node.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Remaining Game"]
        );
        assert_eq!(config.favorites[1].label, "Deleted Game");
        assert!(!config.favorites[1].is_leaf);
    }

    #[test]
    fn position_game_by_identity_prefers_storage_key() {
        let config = Config {
            games: vec![
                test_game("Display Name", "stable-key"),
                test_game("stable-key", "other-key"),
            ],
            ..Config::default()
        };

        assert_eq!(config.position_game_by_identity("stable-key"), Some(0));
    }

    #[test]
    fn position_game_by_identity_falls_back_to_display_name() {
        let config = Config {
            games: vec![test_game("Display Name", "stable-key")],
            ..Config::default()
        };

        assert_eq!(config.position_game_by_identity("Display Name"), Some(0));
    }

    #[test]
    fn missing_apply_confirmation_settings_default_to_enabled() {
        let config: Config = serde_json::from_value(serde_json::json!({
            "version": "1.9.0",
            "backup_path": "save_data",
            "games": [],
            "settings": {}
        }))
        .expect("config without apply confirmation settings should deserialize");

        assert!(config.settings.confirm_before_apply_latest);
        assert!(config.settings.confirm_before_apply_snapshot);
    }
}
