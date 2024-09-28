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
            hotkeys: vec![("CmdOrCtrl+Space".to_string(), QuickActions::Backup)],
        };
        let serialized = serde_json::to_string_pretty(&settings).unwrap();
        println!("{}", serialized);
    }
}
