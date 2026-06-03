use serde::{Deserialize, Serialize};
use specta::Type;

use crate::backup::CompressionPreset;
use crate::cloud_sync::CloudSettings;
use crate::default_value;
use crate::preclude::*;

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct AppearanceSettings {
    #[serde(default = "default_value::default_false")]
    pub custom_font_enabled: bool,
    #[serde(default = "default_value::default")]
    pub ui_font_family: String,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            custom_font_enabled: default_value::default_false(),
            ui_font_family: default_value::default(),
        }
    }
}

/// Settings that can be configured by user
#[derive(Debug, Serialize, Deserialize, Clone, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum SaveListExpandBehavior {
    AlwaysOpen,
    #[default]
    AlwaysClosed,
    RememberLast,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum SaveListSortMode {
    #[default]
    SavedOrder,
    LastPlayed,
    Name,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct Settings {
    #[serde(default = "default_value::default_true")]
    pub prompt_when_not_described: bool,
    #[serde(default = "default_value::default_true")]
    pub extra_backup_when_apply: bool,
    #[serde(default = "default_value::default_true")]
    pub confirm_before_apply_latest: bool,
    #[serde(default = "default_value::default_true")]
    pub confirm_before_apply_snapshot: bool,
    #[serde(default = "default_value::default_false")]
    pub show_edit_button: bool,
    #[serde(default = "default_value::default_true")]
    pub prompt_when_auto_backup: bool,
    #[serde(default = "default_value::default_true")]
    pub exit_to_tray: bool,
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
    #[serde(default = "default_value::default_true")]
    pub log_to_file: bool,
    #[serde(default = "default_value::default_false")]
    pub add_new_to_favorites: bool,
    #[serde(default = "default_value::empty_vec")]
    pub vn_scan_dirs: Vec<String>,
    #[serde(default)]
    pub save_list_expand_behavior: SaveListExpandBehavior,
    #[serde(default = "default_value::default_false")]
    pub save_list_last_expanded: bool,
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
    /// Compute an XXH3 hash when creating snapshots (for integrity verification).
    #[serde(default = "default_value::default_false")]
    pub compute_archive_hash: bool,
    /// Verify archive hash before applying a snapshot.
    #[serde(default = "default_value::default_false")]
    pub verify_archive_before_apply: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            prompt_when_not_described: default_value::default_true(),
            extra_backup_when_apply: default_value::default_true(),
            confirm_before_apply_latest: default_value::default_true(),
            confirm_before_apply_snapshot: default_value::default_true(),
            show_edit_button: default_value::default_false(),
            prompt_when_auto_backup: default_value::default_true(),
            exit_to_tray: default_value::default_true(),
            cloud_settings: CloudSettings::default(),
            locale: default_value::default_locale(),
            default_delete_before_apply: default_value::default_false(),
            default_expend_favorites_tree: default_value::default_false(),
            home_page: default_value::default_home_page(),
            log_to_file: default_value::default_true(),
            add_new_to_favorites: default_value::default_false(),
            vn_scan_dirs: default_value::empty_vec(),
            save_list_expand_behavior: SaveListExpandBehavior::default(),
            save_list_last_expanded: default_value::default_false(),
            save_list_sort_mode: SaveListSortMode::default(),
            save_list_sort_direction: SortDirection::default(),
            max_auto_backup_count: default_value::default_zero_u32(),
            max_extra_backup_count: default_value::default_five_u32(),
            appearance: AppearanceSettings::default(),
            compression_preset: CompressionPreset::default(),
            compute_archive_hash: default_value::default_false(),
            verify_archive_before_apply: default_value::default_false(),
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
