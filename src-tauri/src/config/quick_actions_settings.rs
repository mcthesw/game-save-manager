use serde::{Deserialize, Serialize};

use crate::{backup::Game, default_value};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum QuickActions {
    Apply,
    Backup,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct QuickActionsSettings {
    #[serde(default = "default_value::default_none")]
    pub quick_action_game: Option<Game>,
    #[serde(default = "default_value::empty_vec")]
    pub hotkeys: Vec<(String, QuickActions)>,
}
