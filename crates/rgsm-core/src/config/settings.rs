use serde::{Deserialize, Serialize};
use specta::Type;

use crate::backup::CompressionPreset;
use crate::cloud_sync::CloudSettings;
use crate::default_value;
use crate::preclude::*;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Type, utoipa::ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotTimeFormat {
    #[default]
    Absolute,
    Relative,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Type, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GameListView {
    Favorites,
    All,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, utoipa::ToSchema)]
pub struct AppearanceSettings {
    #[serde(default = "default_value::default_false")]
    pub custom_font_enabled: bool,
    #[serde(default = "default_value::default")]
    pub ui_font_family: String,
    #[serde(default)]
    pub snapshot_time_format: SnapshotTimeFormat,
    #[serde(default)]
    pub default_game_list: Option<GameListView>,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            custom_font_enabled: default_value::default_false(),
            ui_font_family: default_value::default(),
            snapshot_time_format: SnapshotTimeFormat::default(),
            default_game_list: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Type, utoipa::ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum SaveListSortMode {
    #[default]
    SavedOrder,
    LastPlayed,
    Name,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Type, utoipa::ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, utoipa::ToSchema)]
pub struct Settings {
    #[serde(default = "default_value::default_true")]
    pub extra_backup_when_apply: bool,
    #[serde(default = "default_value::default_true")]
    pub confirm_before_apply_latest: bool,
    #[serde(default = "default_value::default_true")]
    pub confirm_before_apply_snapshot: bool,
    #[serde(default = "default_value::default_true")]
    pub prompt_when_auto_backup: bool,
    #[serde(default = "default_value::default_true")]
    pub exit_to_tray: bool,
    #[serde(default = "default_value::default_false")]
    pub auto_check_for_updates: bool,
    #[serde(default = "default_value::default")]
    pub cloud_settings: CloudSettings,
    #[serde(default = "default_value::default_locale")]
    pub locale: String,
    #[serde(default = "default_value::default_false")]
    pub default_delete_before_apply: bool,
    #[serde(default = "default_value::default_false")]
    pub default_expend_favorites_tree: bool,
    #[serde(default = "default_value::default_home_page")]
    pub home_page: String,
    #[serde(default = "default_value::default_false")]
    pub add_new_to_favorites: bool,
    #[serde(default = "default_value::empty_vec")]
    pub vn_scan_dirs: Vec<String>,
    #[serde(default)]
    pub save_list_sort_mode: SaveListSortMode,
    #[serde(default)]
    pub save_list_sort_direction: SortDirection,
    #[serde(default = "default_value::default_zero_u32")]
    pub max_auto_backup_count: u32,
    /// Maximum number of extra overwrite backups to keep per game.
    /// Keep the newest N backups; 0 means unlimited.
    #[serde(default = "default_value::default_five_u32")]
    pub max_extra_backup_count: u32,
    #[serde(default = "default_value::default")]
    pub appearance: AppearanceSettings,
    #[serde(default)]
    pub compression_preset: CompressionPreset,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            extra_backup_when_apply: default_value::default_true(),
            confirm_before_apply_latest: default_value::default_true(),
            confirm_before_apply_snapshot: default_value::default_true(),
            prompt_when_auto_backup: default_value::default_true(),
            exit_to_tray: default_value::default_true(),
            auto_check_for_updates: default_value::default_false(),
            cloud_settings: CloudSettings::default(),
            locale: default_value::default_locale(),
            default_delete_before_apply: default_value::default_false(),
            default_expend_favorites_tree: default_value::default_false(),
            home_page: default_value::default_home_page(),
            add_new_to_favorites: default_value::default_false(),
            vn_scan_dirs: default_value::empty_vec(),
            save_list_sort_mode: SaveListSortMode::default(),
            save_list_sort_direction: SortDirection::default(),
            max_auto_backup_count: default_value::default_zero_u32(),
            max_extra_backup_count: default_value::default_five_u32(),
            appearance: AppearanceSettings::default(),
            compression_preset: CompressionPreset::default(),
        }
    }
}

impl Sanitizable for Settings {
    fn sanitize(self) -> Self {
        Settings {
            cloud_settings: self.cloud_settings.sanitize(),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppearanceSettings;

    #[test]
    fn update_checks_require_opt_in_and_preserve_saved_choice() {
        use super::Settings;
        use crate::config::{Config, ownership::LocalInterfaceSettings};

        assert!(!Settings::default().auto_check_for_updates);
        assert!(!Config::default().settings.auto_check_for_updates);
        let mut old = serde_json::to_value(Settings::default()).unwrap();
        old.as_object_mut()
            .unwrap()
            .remove("auto_check_for_updates");
        assert!(
            !serde_json::from_value::<Settings>(old)
                .unwrap()
                .auto_check_for_updates
        );
        let mut local =
            serde_json::to_value(LocalInterfaceSettings::from(&Settings::default())).unwrap();
        local
            .as_object_mut()
            .unwrap()
            .remove("auto_check_for_updates");
        assert!(
            !serde_json::from_value::<LocalInterfaceSettings>(local)
                .unwrap()
                .auto_check_for_updates
        );
        for enabled in [false, true] {
            let settings = Settings {
                auto_check_for_updates: enabled,
                ..Settings::default()
            };
            let saved = serde_json::to_value(&settings).unwrap();
            assert_eq!(
                serde_json::from_value::<Settings>(saved)
                    .unwrap()
                    .auto_check_for_updates,
                enabled
            );
        }
    }

    #[test]
    fn sidebar_preference_survives_serialization_without_changing_old_defaults() {
        for view in ["favorites", "all"] {
            let settings: AppearanceSettings = serde_json::from_value(serde_json::json!({
                "default_game_list": view
            }))
            .unwrap();
            assert_eq!(
                serde_json::to_value(settings).unwrap()["default_game_list"],
                view
            );
        }
        let old: AppearanceSettings = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(serde_json::to_value(old).unwrap()["default_game_list"].is_null());
    }

    #[test]
    fn snapshot_time_defaults_to_absolute_in_old_appearance_settings() {
        let settings: AppearanceSettings = serde_json::from_value(serde_json::json!({
            "custom_font_enabled": true,
            "ui_font_family": "Example Font"
        }))
        .unwrap();
        let saved = serde_json::to_value(settings).unwrap();
        assert_eq!(saved["snapshot_time_format"], "absolute");
        assert_eq!(saved["ui_font_family"], "Example Font");
    }

    #[test]
    fn snapshot_time_preference_survives_serialization() {
        let settings: AppearanceSettings = serde_json::from_value(serde_json::json!({
            "snapshot_time_format": "relative"
        }))
        .unwrap();
        assert_eq!(
            serde_json::to_value(settings).unwrap()["snapshot_time_format"],
            "relative"
        );
    }
}
