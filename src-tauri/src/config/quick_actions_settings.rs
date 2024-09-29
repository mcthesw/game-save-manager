use serde::{Deserialize, Serialize};

use crate::{backup::Game, default_value};

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct QuickActionsSettings {
    #[serde(default = "default_value::default_none")]
    pub quick_action_game: Option<Game>,
    #[serde(default = "default_value::default")]
    pub hotkeys: QuickActionHotkeys,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn quick_actions_settings_serialize() {
        let settings = QuickActionsSettings {
            quick_action_game: Some(Game {
                name: "test1".to_string(),
                save_paths: vec![],
                game_path: None,
            }),
            hotkeys: QuickActionHotkeys::default(),
        };
        let serialized = serde_json::to_string_pretty(&settings).unwrap();
        println!("{}", serialized);
    }
}
