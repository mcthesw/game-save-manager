use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{backup::Game, default_value};

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QuickActionSoundVariant {
    Default,
    Custom { path: String },
}

impl Default for QuickActionSoundVariant {
    fn default() -> Self {
        QuickActionSoundVariant::Default
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct QuickActionSoundSettings {
    #[serde(default = "default_value::default_true")]
    pub enabled: bool,
    #[serde(default = "default_value::default")]
    pub success: QuickActionSoundVariant,
    #[serde(default = "default_value::default")]
    pub failure: QuickActionSoundVariant,
}

impl Default for QuickActionSoundSettings {
    fn default() -> Self {
        Self {
            enabled: default_value::default_true(),
            success: QuickActionSoundVariant::default(),
            failure: QuickActionSoundVariant::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct QuickActionNotificationSettings {
    #[serde(default = "default_value::default_true")]
    pub enabled: bool,
}

impl Default for QuickActionNotificationSettings {
    fn default() -> Self {
        Self {
            enabled: default_value::default_true(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct QuickActionHotkeys {
    pub apply: Vec<String>,
    pub backup: Vec<String>,
}

impl Default for QuickActionHotkeys {
    fn default() -> Self {
        Self {
            apply: vec!["".to_string(), "".to_string(), "".to_string()],
            backup: vec!["".to_string(), "".to_string(), "".to_string()],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, Type)]
pub struct QuickActionsSettings {
    #[serde(default = "default_value::default_none")]
    pub quick_action_game: Option<Game>,
    #[serde(default = "default_value::default")]
    pub hotkeys: QuickActionHotkeys,
    #[serde(default = "default_value::default")]
    pub notifications: QuickActionNotificationSettings,
    #[serde(default = "default_value::default")]
    pub sound: QuickActionSoundSettings,
}
