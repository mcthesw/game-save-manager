use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{backup::Game, default_value};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum QuickActionSoundKind {
    Success,
    Error,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QuickActionSoundSource {
    SuccessTone,
    ErrorTone,
    File { path: String },
}

impl QuickActionSoundSource {
    pub fn path(&self) -> Option<&str> {
        match self {
            QuickActionSoundSource::File { path } => Some(path.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct QuickActionSoundProfile {
    pub enabled: bool,
    pub source: QuickActionSoundSource,
}

impl QuickActionSoundProfile {
    pub fn success_default() -> Self {
        Self {
            enabled: default_value::default_true(),
            source: QuickActionSoundSource::SuccessTone,
        }
    }

    pub fn error_default() -> Self {
        Self {
            enabled: default_value::default_true(),
            source: QuickActionSoundSource::ErrorTone,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct QuickActionNotificationSettings {
    #[serde(default = "default_value::default_true")]
    pub on_success: bool,
    #[serde(default = "default_value::default_true")]
    pub on_error: bool,
}

impl Default for QuickActionNotificationSettings {
    fn default() -> Self {
        Self {
            on_success: default_value::default_true(),
            on_error: default_value::default_true(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct QuickActionSoundSettings {
    #[serde(default = "QuickActionSoundProfile::success_default")]
    pub success: QuickActionSoundProfile,
    #[serde(default = "QuickActionSoundProfile::error_default")]
    pub error: QuickActionSoundProfile,
    #[serde(default)]
    pub notifications: QuickActionNotificationSettings,
}

impl Default for QuickActionSoundSettings {
    fn default() -> Self {
        Self {
            success: QuickActionSoundProfile::success_default(),
            error: QuickActionSoundProfile::error_default(),
            notifications: QuickActionNotificationSettings::default(),
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
    #[serde(default)]
    pub sound: QuickActionSoundSettings,
}
