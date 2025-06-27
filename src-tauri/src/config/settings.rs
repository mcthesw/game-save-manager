use serde::{Deserialize, Serialize};
use specta::Type;

use crate::cloud_sync::CloudSettings;
use crate::default_value;
use crate::preclude::*;
use crate::sound_notification::SoundSettings;

/// Settings that can be configured by user
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct Settings {
    #[serde(default = "default_value::default_true")]
    pub prompt_when_not_described: bool,
    #[serde(default = "default_value::default_true")]
    pub extra_backup_when_apply: bool,
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
    #[serde(default = "default_value::default")]
    pub sound_settings: SoundSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            prompt_when_not_described: default_value::default_true(),
            extra_backup_when_apply: default_value::default_true(),
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
            sound_settings: SoundSettings::default(),
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
