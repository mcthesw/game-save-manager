use serde::{Deserialize, Serialize};
use smart_default::SmartDefault;
use specta::Type;

use crate::backup::Game;

#[derive(Debug, Serialize, Deserialize, Clone, Type, SmartDefault)]
#[serde(default)]
pub struct QuickActionHotkeys {
    #[default = "vec![String::new(); 3]"]
    pub apply: Vec<String>,
    #[default = "vec![String::new(); 3]"]
    pub backup: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, SmartDefault)]
#[serde(default)]
pub struct QuickActionsSettings {
    #[default]
    pub quick_action_game: Option<Game>,
    #[default]
    pub hotkeys: QuickActionHotkeys,
}
