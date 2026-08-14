use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use log::warn;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::CloudSyncSessionConfig;
use crate::config::get_backup_path;
use crate::device::{DeviceId, get_current_device_id};

const SCHEMA_VERSION: u32 = 1;
const SYNC_STATE_FILE: &str = "sync_state.json";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Type, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SyncResult {
    Success,
    Error(String),
    Conflict,
    Cancelled,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Type, utoipa::ToSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum PendingAction {
    #[default]
    None,
    RetryRequired,
    UserDecisionRequired,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, utoipa::ToSchema, Default)]
pub struct GameSyncState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_known_local_head: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_known_remote_head: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_result: Option<SyncResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<String>,
    #[serde(default)]
    pub pending_action: PendingAction,
}

#[derive(Debug, Serialize, Deserialize, Clone, Type, utoipa::ToSchema)]
pub struct SyncState {
    pub schema_version: u32,
    #[serde(default)]
    pub backend_fingerprint: String,
    #[serde(default)]
    pub current_device_id: DeviceId,
    #[serde(default)]
    pub config_state: GameSyncState,
    #[serde(default)]
    pub games: HashMap<String, GameSyncState>,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            backend_fingerprint: String::new(),
            current_device_id: get_current_device_id().clone(),
            config_state: GameSyncState::default(),
            games: HashMap::new(),
        }
    }
}

pub fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub fn build_game_sync_state(
    local_head: Option<String>,
    remote_head: Option<String>,
    result: SyncResult,
    pending_action: PendingAction,
) -> GameSyncState {
    GameSyncState {
        last_known_local_head: local_head,
        last_known_remote_head: remote_head,
        last_sync_result: Some(result),
        last_sync_at: Some(now_iso8601()),
        pending_action,
    }
}

fn sync_state_path() -> Result<PathBuf, std::io::Error> {
    let backup_path =
        get_backup_path().map_err(|e| std::io::Error::other(format!("config error: {e}")))?;
    Ok(backup_path.join(SYNC_STATE_FILE))
}

pub fn load_sync_state() -> Result<SyncState, std::io::Error> {
    let path = sync_state_path()?;
    if !path.exists() {
        return Ok(SyncState::default());
    }
    let content = fs::read_to_string(&path)?;
    let state: SyncState = serde_json::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(state)
}

pub fn save_sync_state(state: &SyncState) -> Result<(), std::io::Error> {
    let path = sync_state_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(&tmp_path, &json)?;
    if path.exists() {
        fs::remove_file(&path)?;
    }
    fs::rename(&tmp_path, &path)?;
    Ok(())
}

pub fn update_backend_fingerprint(state: &mut SyncState, session: &CloudSyncSessionConfig) {
    state.backend_fingerprint = session.fingerprint();
}

pub fn update_config_sync_state(
    state: &mut SyncState,
    session: &CloudSyncSessionConfig,
    config_state: GameSyncState,
) {
    update_backend_fingerprint(state, session);
    state.config_state = config_state;
}

pub fn update_game_sync_state(
    state: &mut SyncState,
    session: &CloudSyncSessionConfig,
    game_name: &str,
    game_state: GameSyncState,
) {
    update_backend_fingerprint(state, session);
    state.games.insert(game_name.to_string(), game_state);
}

static SYNC_STATE_LOCK: Mutex<()> = Mutex::new(());

pub fn with_sync_state<F>(f: F) -> Result<(), std::io::Error>
where
    F: FnOnce(&mut SyncState),
{
    let _guard = SYNC_STATE_LOCK
        .lock()
        .map_err(|e| std::io::Error::other(format!("sync state lock poisoned: {e}")))?;

    let mut state = match load_sync_state() {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to load sync_state.json, resetting to default: {e}");
            SyncState::default()
        }
    };
    f(&mut state);
    save_sync_state(&state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_state_default_has_expected_fields() {
        let state = SyncState::default();
        assert_eq!(state.schema_version, SCHEMA_VERSION);
        assert!(state.games.is_empty());
        assert!(state.backend_fingerprint.is_empty());
        assert!(state.current_device_id.len() > 1);
    }

    #[test]
    fn build_game_sync_state_sets_result_and_timestamp() {
        let state = build_game_sync_state(
            Some("local".into()),
            Some("remote".into()),
            SyncResult::Success,
            PendingAction::None,
        );
        assert_eq!(state.last_known_local_head.as_deref(), Some("local"));
        assert_eq!(state.last_known_remote_head.as_deref(), Some("remote"));
        assert_eq!(state.last_sync_result, Some(SyncResult::Success));
        assert!(state.last_sync_at.is_some());
        assert_eq!(state.pending_action, PendingAction::None);
    }

    #[test]
    fn update_config_sync_state_updates_config_row_and_fingerprint() {
        let session = CloudSyncSessionConfig {
            root_path: "/root".into(),
            max_concurrency: 1,
            backend: crate::cloud_sync::Backend::Disabled,
        };
        let mut state = SyncState::default();
        let config_state = build_game_sync_state(
            Some("local-head".into()),
            Some("remote-head".into()),
            SyncResult::Success,
            PendingAction::None,
        );

        update_config_sync_state(&mut state, &session, config_state.clone());

        assert_eq!(
            state.config_state.last_sync_result,
            Some(SyncResult::Success)
        );
        assert_eq!(
            state.config_state.last_known_local_head.as_deref(),
            Some("local-head")
        );
        assert_eq!(
            state.config_state.last_known_remote_head.as_deref(),
            Some("remote-head")
        );
        assert_eq!(state.backend_fingerprint, session.fingerprint());
    }

    #[test]
    fn update_game_sync_state_overwrites_existing_entry() {
        let session = CloudSyncSessionConfig {
            root_path: "/root".into(),
            max_concurrency: 1,
            backend: crate::cloud_sync::Backend::Disabled,
        };
        let mut state = SyncState::default();
        update_game_sync_state(
            &mut state,
            &session,
            "Game1",
            build_game_sync_state(None, None, SyncResult::Success, PendingAction::None),
        );
        update_game_sync_state(
            &mut state,
            &session,
            "Game1",
            build_game_sync_state(
                None,
                None,
                SyncResult::Conflict,
                PendingAction::UserDecisionRequired,
            ),
        );
        assert_eq!(
            state.games["Game1"].last_sync_result,
            Some(SyncResult::Conflict)
        );
        assert_eq!(
            state.games["Game1"].pending_action,
            PendingAction::UserDecisionRequired
        );
        assert_eq!(state.backend_fingerprint, session.fingerprint());
    }
}
