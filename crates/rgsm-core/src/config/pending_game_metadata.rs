use serde::{Deserialize, Serialize};

use super::{LocalState, SharedGame};

/// An edit belongs to LocalState's connected library, alongside its cached base.
/// Device settings and shared retention are deliberately outside this record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, utoipa::ToSchema)]
pub struct PendingGameMetadata {
    pub base: Option<SharedGame>,
    pub desired: SharedGame,
    #[serde(default)]
    pub conflict: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MetadataDecision {
    Publish,
    Accept,
    Conflict,
}

impl PendingGameMetadata {
    pub fn decision(&self, remote: Option<&SharedGame>) -> MetadataDecision {
        let base = self.base.as_ref().map(SharedGame::normalized_portable);
        let desired = self.desired.normalized_portable();
        let remote = remote.map(SharedGame::normalized_portable);
        if remote.as_ref() == Some(&desired) || base.as_ref() == Some(&desired) {
            MetadataDecision::Accept
        } else if remote == base {
            MetadataDecision::Publish
        } else {
            MetadataDecision::Conflict
        }
    }
}

impl LocalState {
    pub(crate) fn pending_definitions(&self) -> Vec<SharedGame> {
        self.pending_game_metadata
            .values()
            .map(|edit| edit.desired.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ConfigurationOwners};

    fn game(name: &str) -> SharedGame {
        serde_json::from_value(serde_json::json!({
            "name": name, "storage_key": "game", "save_units": [], "next_save_unit_id": 0
        }))
        .unwrap()
    }

    #[test]
    fn compares_per_game_and_accepts_identical_results() {
        let edit = PendingGameMetadata {
            base: Some(game("Base")),
            desired: game("Local"),
            conflict: false,
        };
        assert_eq!(
            edit.decision(Some(&game("Base"))),
            MetadataDecision::Publish
        );
        assert_eq!(
            edit.decision(Some(&game("Local"))),
            MetadataDecision::Accept
        );
        assert_eq!(
            edit.decision(Some(&game("Remote"))),
            MetadataDecision::Conflict
        );
        assert_eq!(edit.decision(None), MetadataDecision::Conflict);
        let added = PendingGameMetadata { base: None, ..edit };
        assert_eq!(added.decision(None), MetadataDecision::Publish);
    }

    #[test]
    fn old_state_has_no_pending_edits_and_records_survive_restart() {
        let owners = ConfigurationOwners::from_legacy(&Config::default(), &"device".to_string());
        let bytes = serde_json::to_vec(&owners.local_state).unwrap();
        let mut state: LocalState = serde_json::from_slice(&bytes).unwrap();
        assert!(state.pending_game_metadata.is_empty());
        state.pending_game_metadata.insert(
            "game".to_string(),
            PendingGameMetadata {
                base: None,
                desired: game("Offline"),
                conflict: false,
            },
        );
        let reloaded: LocalState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(reloaded.pending_game_metadata, state.pending_game_metadata);
    }
}
