use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{backup::Game, default_value};

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

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QuickActionSoundSource {
    Default,
    File { path: String },
}

impl Default for QuickActionSoundSource {
    fn default() -> Self {
        Self::Default
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, Type)]
pub struct QuickActionSoundSlots {
    #[serde(default)]
    pub success: QuickActionSoundSource,
    #[serde(default)]
    pub failure: QuickActionSoundSource,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct QuickActionSoundPreferences {
    #[serde(default = "default_value::default_true")]
    pub enable_sound: bool,
    #[serde(default)]
    pub sounds: QuickActionSoundSlots,
}

impl Default for QuickActionSoundPreferences {
    fn default() -> Self {
        Self {
            enable_sound: default_value::default_true(),
            sounds: QuickActionSoundSlots::default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct QuickActionsSettings {
    #[serde(default = "default_value::default_none")]
    pub quick_action_game: Option<Game>,
    #[serde(default = "default_value::default")]
    pub hotkeys: QuickActionHotkeys,
    #[serde(default = "default_value::default_true")]
    pub enable_sound: bool,
    #[serde(default = "default_value::default_true")]
    pub enable_notification: bool,
    #[serde(default)]
    pub sounds: QuickActionSoundSlots,
}

impl Default for QuickActionsSettings {
    fn default() -> Self {
        Self {
            quick_action_game: default_value::default_none(),
            hotkeys: QuickActionHotkeys::default(),
            enable_sound: default_value::default_true(),
            enable_notification: default_value::default_true(),
            sounds: QuickActionSoundSlots::default(),
        }
    }
}

impl From<&QuickActionsSettings> for QuickActionSoundPreferences {
    fn from(value: &QuickActionsSettings) -> Self {
        Self {
            enable_sound: value.enable_sound,
            sounds: value.sounds.clone(),
        }
    }
}
