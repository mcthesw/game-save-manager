use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;

use crate::{backup::Game, default_value};

#[derive(Deserialize)]
#[serde(untagged)]
enum QuickActionGameReference {
    Id(String),
    LegacyGame(Box<Game>),
}

fn deserialize_quick_action_game_id<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(
        Option::<QuickActionGameReference>::deserialize(deserializer)?.map(|reference| {
            match reference {
                QuickActionGameReference::Id(id) => id,
                QuickActionGameReference::LegacyGame(game) if !game.storage_key.is_empty() => {
                    game.storage_key
                }
                QuickActionGameReference::LegacyGame(game) => game.name,
            }
        }),
    )
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type)]
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

#[derive(Debug, Serialize, Deserialize, Clone, Type, Default, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QuickActionSoundSource {
    #[default]
    Default,
    File {
        path: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, Type, PartialEq, Eq)]
pub struct QuickActionSoundSlots {
    #[serde(default)]
    pub success: QuickActionSoundSource,
    #[serde(default)]
    pub failure: QuickActionSoundSource,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, PartialEq, Eq)]
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

#[derive(Debug, Serialize, Deserialize, Clone, Type, PartialEq, Eq)]
pub struct QuickActionsSettings {
    #[serde(
        default = "default_value::default_none",
        alias = "quick_action_game",
        deserialize_with = "deserialize_quick_action_game_id"
    )]
    pub quick_action_game_id: Option<String>,
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
            quick_action_game_id: default_value::default_none(),
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
    pub fn selected_game<'a>(&self, games: &'a [Game]) -> Option<&'a Game> {
        let identity = self.quick_action_game_id.as_deref()?;
        games
            .iter()
            .find(|game| !identity.is_empty() && game.storage_key == identity)
            .or_else(|| games.iter().find(|game| game.name == identity))
    }

    pub fn remove_deleted_game_reference(&mut self, deleted_game: &Game) -> bool {
        self.remove_game_reference(&deleted_game.storage_key, &deleted_game.name)
    }

    pub(crate) fn remove_game_reference(&mut self, game_id: &str, game_name: &str) -> bool {
        let should_clear = self
            .quick_action_game_id
            .as_deref()
            .is_some_and(|identity| identity == game_id || identity == game_name);

        if should_clear {
            self.quick_action_game_id = None;
        }

        let before_len = self.game_automations.len();
        self.game_automations
            .retain(|automation| !automation.references_game_identity(game_id, game_name));

        should_clear || before_len != self.game_automations.len()
    }

    pub(crate) fn references_game_identity(&self, game_id: &str, game_name: &str) -> bool {
        self.quick_action_game_id
            .as_deref()
            .is_some_and(|identity| identity == game_id || identity == game_name)
            || self
                .game_automations
                .iter()
                .any(|automation| automation.references_game_identity(game_id, game_name))
    }

    pub fn sync_updated_game_reference(&mut self, previous_game: &Game, updated_game: &Game) {
        if let Some(identity) = self.quick_action_game_id.as_deref()
            && ((!previous_game.storage_key.is_empty() && identity == previous_game.storage_key)
                || identity == previous_game.name)
        {
            self.quick_action_game_id = Some(if updated_game.storage_key.is_empty() {
                updated_game.name.clone()
            } else {
                updated_game.storage_key.clone()
            });
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

#[cfg(test)]
mod tests {
    use super::*;

    fn game(name: &str, storage_key: &str) -> Game {
        Game {
            name: name.to_string(),
            storage_key: storage_key.to_string(),
            save_paths: Vec::new(),
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: false,
            auto_backup: None,
            ludusavi_meta: None,
            device_bindings: Default::default(),
        }
    }

    #[test]
    fn legacy_game_reference_deserializes_to_storage_key() {
        let legacy = serde_json::json!({
            "quick_action_game": game("Display Name", "stable-key")
        });

        let settings: QuickActionsSettings = serde_json::from_value(legacy).unwrap();

        assert_eq!(settings.quick_action_game_id.as_deref(), Some("stable-key"));
    }

    #[test]
    fn serialization_emits_only_stable_game_id() {
        let settings = QuickActionsSettings {
            quick_action_game_id: Some("stable-key".to_string()),
            ..Default::default()
        };

        let serialized = serde_json::to_value(settings).unwrap();

        assert_eq!(
            serialized.pointer("/quick_action_game_id"),
            Some(&serde_json::Value::String("stable-key".to_string()))
        );
        assert!(serialized.get("quick_action_game").is_none());
    }

    #[test]
    fn selected_game_accepts_legacy_display_name() {
        let games = vec![game("Display Name", "stable-key")];
        let settings = QuickActionsSettings {
            quick_action_game_id: Some("Display Name".to_string()),
            ..Default::default()
        };

        assert_eq!(
            settings
                .selected_game(&games)
                .map(|game| game.storage_key.as_str()),
            Some("stable-key")
        );
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

#[derive(Debug, Serialize, Deserialize, Clone, Type, PartialEq, Eq)]
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
        self.references_game_identity(&game.storage_key, &game.name)
    }

    fn references_game_identity(&self, game_id: &str, game_name: &str) -> bool {
        if !self.storage_key.is_empty() && !game_id.is_empty() {
            return self.storage_key == game_id;
        }
        self.game_name == game_name
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
