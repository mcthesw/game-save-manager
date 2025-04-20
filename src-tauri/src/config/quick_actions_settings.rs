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

#[derive(Debug, Serialize, Deserialize, Clone, Default, Type)]
pub struct QuickActionsSettings {
    #[serde(default = "default_value::default_none")]
    pub quick_action_game: Option<Game>,
    #[serde(default = "default_value::default")]
    pub hotkeys: QuickActionHotkeys,
}
