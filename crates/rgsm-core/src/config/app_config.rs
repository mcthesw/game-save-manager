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
#[derive(Debug, Serialize, Deserialize, Clone, Type, utoipa::ToSchema)]
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
        self.bind_legacy_game_references();
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
        super::game_identity::position_game_by_identity(&self.games, identity)
    }

    pub(crate) fn bind_legacy_game_references(&mut self) {
        FavoriteTreeNode::bind_legacy_games(&mut self.favorites, &self.games);
        if let Some(game) = self.quick_action.selected_game(&self.games)
            && !game.storage_key.is_empty()
        {
            self.quick_action.quick_action_game_id = Some(game.storage_key.clone());
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type, utoipa::ToSchema)]
pub struct FavoriteTreeNode {
    node_id: String,
    label: String,
    is_leaf: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    game_id: Option<String>,
    #[schema(no_recursion)]
    children: Option<Vec<Self>>,
}

impl FavoriteTreeNode {
    fn bind_legacy_games(nodes: &mut [Self], games: &[Game]) {
        for node in nodes {
            if node.is_leaf && node.game_id.is_none() {
                let mut matches = games.iter().filter(|game| game.name == node.label);
                if let Some(game) = matches.next()
                    && matches.next().is_none()
                    && !game.storage_key.is_empty()
                {
                    node.game_id = Some(game.storage_key.clone());
                }
            }
            if let Some(children) = &mut node.children {
                Self::bind_legacy_games(children, games);
            }
        }
    }

    pub(crate) fn rename_game_leaves(nodes: &mut [Self], game_id: &str, next: &str) {
        for node in nodes {
            if node.is_leaf && node.game_id.as_deref() == Some(game_id) {
                node.label = next.to_string();
            }
            if let Some(children) = &mut node.children {
                Self::rename_game_leaves(children, game_id, next);
            }
        }
    }

    fn remove_deleted_game_leaves(nodes: &mut Vec<Self>, deleted_game: &Game) -> bool {
        Self::remove_game_leaves(nodes, &deleted_game.storage_key)
    }

    pub(crate) fn remove_game_leaves(nodes: &mut Vec<Self>, game_id: &str) -> bool {
        let mut changed = false;

        nodes.retain_mut(|node| {
            if node.is_leaf && node.game_id.as_deref() == Some(game_id) {
                changed = true;
                return false;
            }

            if let Some(children) = &mut node.children
                && Self::remove_game_leaves(children, game_id)
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
            game_id: None,
            children: None,
        }
    }

    fn favorite_folder(label: &str, children: Vec<FavoriteTreeNode>) -> FavoriteTreeNode {
        FavoriteTreeNode {
            node_id: format!("folder-{label}"),
            label: label.to_string(),
            is_leaf: false,
            game_id: None,
            children: Some(children),
        }
    }

    #[test]
    fn cleanup_deleted_game_references_removes_matching_favorite_leaves() {
        let deleted_game = test_game("Deleted Game", "deleted-game-key");
        let mut config = Config {
            games: vec![
                deleted_game.clone(),
                test_game("Remaining Game", "remaining"),
            ],
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
    fn renaming_game_updates_nested_favorites_without_renaming_folders() {
        let mut nodes = vec![favorite_folder(
            "Before",
            vec![favorite_leaf("Before"), favorite_leaf("Other")],
        )];
        FavoriteTreeNode::bind_legacy_games(&mut nodes, &[test_game("Before", "stable")]);
        FavoriteTreeNode::rename_game_leaves(&mut nodes, "stable", "After");
        assert_eq!(nodes[0].label, "Before");
        let children = nodes[0].children.as_ref().unwrap();
        assert_eq!(children[0].label, "After");
        assert_eq!(children[1].label, "Other");
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
    fn game_references_reject_ambiguous_legacy_names() {
        let config = Config {
            games: vec![test_game("Same", "first"), test_game("Same", "second")],
            ..Default::default()
        };
        assert_eq!(config.position_game_by_identity("Same"), None);
        assert_eq!(config.position_game_by_identity("second"), Some(1));
    }

    #[test]
    fn game_references_delete_only_the_matching_favorite_identity() {
        let deleted = test_game("Same", "first");
        let mut config = Config {
            games: vec![deleted.clone(), test_game("Same", "second")],
            favorites: serde_json::from_value(serde_json::json!([
                {"node_id":"a", "label":"Same", "is_leaf":true, "game_id":"first"},
                {"node_id":"b", "label":"Same", "is_leaf":true, "game_id":"second"}
            ]))
            .unwrap(),
            ..Default::default()
        };
        assert!(config.remove_deleted_game_references(&deleted));
        assert_eq!(config.favorites.len(), 1);
        assert_eq!(config.favorites[0].node_id, "b");
    }

    #[test]
    fn game_references_bind_legacy_favorites_before_splitting_owners() {
        let config = Config {
            games: vec![test_game("Legacy", "legacy-id")],
            favorites: vec![favorite_folder("Folder", vec![favorite_leaf("Legacy")])],
            ..Default::default()
        };
        let owners = crate::config::ConfigurationOwners::from_legacy(&config, &"device".into());
        let favorites =
            serde_json::to_value(&owners.device_profiles["device"].private_favorites).unwrap();
        assert_eq!(favorites[0]["children"][0]["game_id"], "legacy-id");
    }

    #[test]
    fn game_references_do_not_guess_ambiguous_or_stale_favorites() {
        let mut config = Config {
            games: vec![
                test_game("Same", "first"),
                test_game("Same", "second"),
                test_game("Unique", "unique"),
            ],
            favorites: serde_json::from_value(serde_json::json!([
                {"node_id":"ambiguous", "label":"Same", "is_leaf":true},
                {"node_id":"stale", "label":"Unique", "is_leaf":true, "game_id":"removed"},
                {"node_id":"legacy", "label":"Unique", "is_leaf":true}
            ]))
            .unwrap(),
            ..Default::default()
        };
        config.bind_legacy_game_references();
        assert_eq!(config.favorites[0].game_id, None);
        assert_eq!(config.favorites[1].game_id.as_deref(), Some("removed"));
        assert_eq!(config.favorites[2].game_id.as_deref(), Some("unique"));
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
