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

#[derive(Debug, Serialize, Deserialize, Clone, Type, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QuickActionSoundSource {
    #[default]
    Default,
    File {
        path: String,
    },
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
    #[serde(default = "default_value::default_true")]
    pub notify_when_unchanged: bool,
    #[serde(default)]
    pub sounds: QuickActionSoundSlots,
    #[serde(default = "default_value::empty_vec")]
    pub game_automations: Vec<GameAutomationSettings>,
}

impl Default for QuickActionsSettings {
    fn default() -> Self {
        Self {
            quick_action_game: default_value::default_none(),
            hotkeys: QuickActionHotkeys::default(),
            enable_sound: default_value::default_true(),
            enable_notification: default_value::default_true(),
            notify_when_unchanged: default_value::default_true(),
            sounds: QuickActionSoundSlots::default(),
            game_automations: default_value::empty_vec(),
        }
    }
}

impl QuickActionsSettings {
    pub fn remove_deleted_game_reference(&mut self, deleted_game: &Game) -> bool {
        let should_clear = self
            .quick_action_game
            .as_ref()
            .is_some_and(|game| game.references_same_game(deleted_game));

        if should_clear {
            self.quick_action_game = None;
        }

        let before_len = self.game_automations.len();
        self.game_automations
            .retain(|automation| !automation.references_game(deleted_game));

        should_clear || before_len != self.game_automations.len()
    }

    pub fn sync_updated_game_reference(&mut self, previous_game: &Game, updated_game: &Game) {
        if let Some(ref mut qa_game) = self.quick_action_game
            && qa_game.references_same_game(previous_game)
        {
            qa_game.name = updated_game.name.clone();
            qa_game.storage_key = updated_game.storage_key.clone();
        }

        for automation in &mut self.game_automations {
            if automation.references_game(previous_game) {
                automation.game_name = updated_game.name.clone();
                automation.storage_key = updated_game.storage_key.clone();
            }
        }
    }

    pub fn automation_for_game(&self, game: &Game) -> Option<&GameAutomationSettings> {
        self.game_automations
            .iter()
            .find(|automation| automation.references_game(game))
    }

    pub fn upsert_game_automation(&mut self, game: &Game, draft: GameAutomationSettingsDraft) {
        let automation = GameAutomationSettings {
            storage_key: game.storage_key.clone(),
            game_name: game.name.clone(),
            process_name: draft.process_name,
            on_process_start: draft.on_process_start,
            on_process_exit: draft.on_process_exit,
            in_process_interval_secs: draft.in_process_interval_secs,
        };

        if let Some(existing) = self
            .game_automations
            .iter_mut()
            .find(|existing| existing.references_game(game))
        {
            *existing = automation;
        } else {
            self.game_automations.push(automation);
        }
    }

    pub fn remove_game_automation(&mut self, game: &Game) -> bool {
        let before_len = self.game_automations.len();
        self.game_automations
            .retain(|automation| !automation.references_game(game));
        before_len != self.game_automations.len()
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

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct GameAutomationSettings {
    #[serde(default)]
    pub storage_key: String,
    pub game_name: String,
    #[serde(default)]
    pub process_name: String,
    #[serde(default)]
    pub on_process_start: bool,
    #[serde(default)]
    pub on_process_exit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_process_interval_secs: Option<u32>,
}

impl GameAutomationSettings {
    pub fn references_game(&self, game: &Game) -> bool {
        if !self.storage_key.is_empty() && !game.storage_key.is_empty() {
            return self.storage_key == game.storage_key;
        }

        self.game_name == game.name
    }

    pub fn has_process_triggers(&self) -> bool {
        self.on_process_start || self.on_process_exit || self.in_process_interval_secs.is_some()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Type)]
pub struct GameAutomationSettingsDraft {
    #[serde(default)]
    pub process_name: String,
    #[serde(default)]
    pub on_process_start: bool,
    #[serde(default)]
    pub on_process_exit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub in_process_interval_secs: Option<u32>,
}

impl GameAutomationSettingsDraft {
    pub fn has_process_triggers(&self) -> bool {
        self.on_process_start || self.on_process_exit || self.in_process_interval_secs.is_some()
    }
}
