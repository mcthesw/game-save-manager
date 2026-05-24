use log::info;
use opendal::Operator;
use serde::{Deserialize, Serialize};
use specta::Type;

use super::CloudSyncSessionConfig;
use super::conflict::{ConflictResolution, SyncRelation, determine_sync_relation};
use super::state_recording::{
    current_device_head, log_config_sync_failure, log_game_sync_failure, merged_error_state,
    record_config_state, record_game_state,
};
use super::sync_state::{GameSyncState, PendingAction, SyncResult, load_sync_state};
use super::utils::{
    SyncOperationError, load_remote_game_snapshots, replace_local_game_with_remote,
    upload_config_snapshot, upload_game_data,
};
use crate::backup::{Game, GameSnapshots};
use crate::config::Config;
use crate::preclude::BackendError;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolutionOutcome {
    Cancelled,
    KeptLocal,
    AcceptedRemote,
}

struct ConflictContext {
    previous_state: GameSyncState,
    local: GameSnapshots,
    remote: GameSnapshots,
}

fn sync_error_to_backend(err: SyncOperationError) -> BackendError {
    match err {
        SyncOperationError::Cancelled => BackendError::Cancelled,
        SyncOperationError::Backend(inner) => inner,
    }
}

fn require_unresolved_conflict_state(game_name: &str) -> Result<GameSyncState, BackendError> {
    let state = load_sync_state()?;
    let Some(game_state) = state.games.get(game_name) else {
        return Err(BackendError::InvalidConflictState(game_name.to_string()));
    };
    if game_state.pending_action != PendingAction::UserDecisionRequired {
        return Err(BackendError::InvalidConflictState(game_name.to_string()));
    }
    Ok(game_state.clone())
}

async fn load_remote_snapshots(
    op: &Operator,
    game: &Game,
) -> Result<Option<GameSnapshots>, BackendError> {
    load_remote_game_snapshots(op, &game.backup_dir_name(), None)
        .await
        .map_err(sync_error_to_backend)
}

async fn load_conflict_context(
    op: &Operator,
    game: &Game,
) -> Result<ConflictContext, BackendError> {
    let previous_state = require_unresolved_conflict_state(&game.name)?;
    let local = game.get_game_snapshots_info()?;
    let Some(remote) = load_remote_snapshots(op, game).await? else {
        return Err(BackendError::InvalidConflictState(game.name.clone()));
    };

    if determine_sync_relation(&local, &remote) != SyncRelation::IncompatibleState {
        return Err(BackendError::InvalidConflictState(game.name.clone()));
    }

    Ok(ConflictContext {
        previous_state,
        local,
        remote,
    })
}

fn record_resolution_failure(
    session: &CloudSyncSessionConfig,
    game: &Game,
    context: &ConflictContext,
    operation: &str,
    backend_err: &BackendError,
) {
    let message = backend_err.to_string();
    let (local_head, remote_head, result, pending) = merged_error_state(
        Some(&context.previous_state),
        current_device_head(&context.local),
        current_device_head(&context.remote),
        message.clone(),
        PendingAction::UserDecisionRequired,
    );
    record_game_state(
        session,
        &game.name,
        local_head,
        remote_head,
        result,
        pending,
    );
    log_game_sync_failure(
        session,
        &game.name,
        operation,
        PendingAction::UserDecisionRequired,
        &message,
    );
}

pub async fn resolve_game_conflict(
    session: &CloudSyncSessionConfig,
    op: &Operator,
    game: &Game,
    resolution: ConflictResolution,
) -> Result<ConflictResolutionOutcome, BackendError> {
    match resolution {
        ConflictResolution::Cancelled => {
            info!(
                target: "rgsm::cloud::recovery",
                "Conflict resolution cancelled for game {}",
                game.name
            );
            Ok(ConflictResolutionOutcome::Cancelled)
        }
        ConflictResolution::Fork => {
            // TODO: fork should preserve both branch heads without merging, like a git branch.
            Err(BackendError::UnsupportedConflictResolution("fork".into()))
        }
        ConflictResolution::KeepLocal | ConflictResolution::AcceptRemote => {
            let context = load_conflict_context(op, game).await?;
            match resolution {
                ConflictResolution::KeepLocal => {
                    keep_local_progress(session, op, game, &context).await
                }
                ConflictResolution::AcceptRemote => {
                    accept_remote_progress(session, op, game, &context).await
                }
                ConflictResolution::Cancelled | ConflictResolution::Fork => unreachable!(),
            }
        }
    }
}

async fn keep_local_progress(
    session: &CloudSyncSessionConfig,
    op: &Operator,
    game: &Game,
    context: &ConflictContext,
) -> Result<ConflictResolutionOutcome, BackendError> {
    let result = upload_game_data(op, game, session.normalized_max_concurrency(), None).await;

    match result {
        Ok(uploaded) => {
            let head = current_device_head(&uploaded);
            record_game_state(
                session,
                &game.name,
                head.clone(),
                head,
                SyncResult::Success,
                PendingAction::None,
            );
            Ok(ConflictResolutionOutcome::KeptLocal)
        }
        Err(err) => {
            let backend_err = sync_error_to_backend(err);
            record_resolution_failure(session, game, context, "resolve_keep_local", &backend_err);
            Err(backend_err)
        }
    }
}

async fn accept_remote_progress(
    session: &CloudSyncSessionConfig,
    op: &Operator,
    game: &Game,
    context: &ConflictContext,
) -> Result<ConflictResolutionOutcome, BackendError> {
    let result = replace_local_game_with_remote(
        session,
        game,
        op,
        "game-conflict-accept-remote-stage",
        None,
    )
    .await;

    match result {
        Ok(downloaded) => {
            let remote_head = current_device_head(&downloaded);
            record_game_state(
                session,
                &game.name,
                remote_head.clone(),
                remote_head,
                SyncResult::Success,
                PendingAction::None,
            );
            Ok(ConflictResolutionOutcome::AcceptedRemote)
        }
        Err(err) => {
            let backend_err = sync_error_to_backend(err);
            record_resolution_failure(
                session,
                game,
                context,
                "resolve_accept_remote",
                &backend_err,
            );
            Err(backend_err)
        }
    }
}

pub async fn sync_config(
    session: &CloudSyncSessionConfig,
    op: &Operator,
    config: &Config,
) -> Result<(), BackendError> {
    match upload_config_snapshot(op, config).await {
        Ok(()) => {
            record_config_state(session, SyncResult::Success, PendingAction::None);
            Ok(())
        }
        Err(err) => {
            let message = err.to_string();
            record_config_state(
                session,
                SyncResult::Error(message.clone()),
                PendingAction::RetryRequired,
            );
            log_config_sync_failure(session, "manual_config_upload", &message);
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use opendal::Operator;
    use opendal::services;
    use temp_dir::TempDir;

    use super::*;
    use crate::backup::Snapshot;
    use crate::cloud_sync::{Backend, CloudSyncSessionConfig};
    use crate::config::{Config, Settings};
    use crate::device::get_current_device_id;

    struct ConfigFileGuard {
        path: PathBuf,
        original: Option<Vec<u8>>,
    }

    impl ConfigFileGuard {
        fn write(config: &Config) -> Self {
            let path = crate::app_dirs::resolve_app_path("GameSaveManager.config.json");
            let original = fs::read(&path).ok();
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("config parent should be created");
            }
            fs::write(&path, serde_json::to_vec_pretty(config).unwrap())
                .expect("config should be written");
            Self { path, original }
        }
    }

    impl Drop for ConfigFileGuard {
        fn drop(&mut self) {
            if let Some(original) = &self.original {
                let _ = fs::write(&self.path, original);
            } else {
                let _ = fs::remove_file(&self.path);
            }
        }
    }

    fn memory_operator() -> Operator {
        Operator::new(services::Memory::default()).unwrap().finish()
    }

    fn test_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    fn session() -> CloudSyncSessionConfig {
        CloudSyncSessionConfig {
            root_path: "/test-root".into(),
            max_concurrency: 1,
            backend: Backend::Disabled,
        }
    }

    fn game() -> Game {
        Game {
            name: "Test Game".to_string(),
            storage_key: "test-game".to_string(),
            save_paths: Vec::new(),
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            ludusavi_meta: None,
            store_user_ids: Default::default(),
        }
    }

    fn snapshot(date: &str, parent: Option<&str>, path: String) -> Snapshot {
        Snapshot {
            date: date.to_string(),
            describe: String::new(),
            path,
            size: 0,
            parent: parent.map(str::to_string),
            archive_hash: None,
            device_id: Some(get_current_device_id().clone()),
            created_by: Default::default(),
        }
    }

    fn snapshots(name: &str, items: Vec<Snapshot>, head: &str) -> GameSnapshots {
        let mut snapshots = GameSnapshots::new(name);
        snapshots.backups = items;
        snapshots.set_current_device_head(Some(head.to_string()));
        snapshots
    }

    fn write_local_snapshots(backup_root: &Path, game: &Game, dates: &[&str]) -> GameSnapshots {
        let game_dir = backup_root.join(game.backup_dir_name().as_ref());
        fs::create_dir_all(&game_dir).expect("game directory should be created");
        let items = dates
            .iter()
            .enumerate()
            .map(|(index, date)| {
                let zip_path = game_dir.join(format!("{date}.zip"));
                fs::write(&zip_path, format!("local {date}")).expect("zip should be written");
                let parent = index.checked_sub(1).map(|previous| dates[previous]);
                snapshot(date, parent, zip_path.to_string_lossy().to_string())
            })
            .collect::<Vec<_>>();
        let info = snapshots(&game.name, items, dates.last().unwrap());
        fs::write(
            game_dir.join("Backups.json"),
            serde_json::to_vec_pretty(&info).unwrap(),
        )
        .expect("metadata should be written");
        info
    }

    async fn write_remote_snapshots(op: &Operator, storage_key: &str, info: &GameSnapshots) {
        write_remote_metadata(op, storage_key, info).await;
        for snapshot in &info.backups {
            let zip_path = super::super::utils::game_cloud_zip_path(storage_key, &snapshot.date)
                .expect("remote zip path should build");
            op.write(&zip_path, format!("remote {}", snapshot.date))
                .await
                .expect("remote zip should be written");
        }
    }

    async fn write_remote_metadata(op: &Operator, storage_key: &str, info: &GameSnapshots) {
        let metadata_path = super::super::utils::game_cloud_metadata_path(storage_key).unwrap();
        op.write(&metadata_path, serde_json::to_vec_pretty(info).unwrap())
            .await
            .expect("remote metadata should be written");
    }

    fn write_conflict_state(session: &CloudSyncSessionConfig, game: &Game) {
        record_game_state(
            session,
            &game.name,
            Some("local".into()),
            Some("remote".into()),
            SyncResult::Conflict,
            PendingAction::UserDecisionRequired,
        );
    }

    fn read_game_state(game: &Game) -> super::super::sync_state::GameSyncState {
        super::super::sync_state::load_sync_state()
            .expect("sync state should load")
            .games
            .get(&game.name)
            .cloned()
            .expect("game state should exist")
    }

    fn config_for(backup_root: &Path, game: &Game) -> Config {
        Config {
            backup_path: backup_root.to_string_lossy().to_string(),
            games: vec![game.clone()],
            settings: Settings {
                cloud_settings: super::super::CloudSettings {
                    backend: Backend::Disabled,
                    root_path: "/test-root".into(),
                    max_concurrency: 1,
                    auto_sync_interval: 0,
                },
                ..Settings::default()
            },
            ..Config::default()
        }
    }

    #[test]
    fn incompatible_sync_relation_records_user_decision_required() {
        let _lock = crate::config::lock_config_test_file();
        let temp = TempDir::new().expect("temp dir should be created");
        let game = game();
        let backup_root = temp.path().join("backup");
        write_local_snapshots(&backup_root, &game, &["base", "local"]);
        let config = config_for(&backup_root, &game);
        let _guard = ConfigFileGuard::write(&config);
        let session = session();
        let op = memory_operator();
        let remote = snapshots(
            &game.name,
            vec![
                snapshot("base", None, String::new()),
                snapshot("cloud", Some("base"), String::new()),
            ],
            "cloud",
        );
        test_runtime().block_on(write_remote_snapshots(&op, "test-game", &remote));

        let result = test_runtime()
            .block_on(super::super::facade::sync_game(&session, &op, &game))
            .unwrap();

        assert_eq!(result, super::super::SyncGameOutcome::Conflict);
        let state = read_game_state(&game);
        assert_eq!(state.last_sync_result, Some(SyncResult::Conflict));
        assert_eq!(state.pending_action, PendingAction::UserDecisionRequired);
    }

    #[test]
    fn cancelled_resolution_preserves_conflict_state() {
        let _lock = crate::config::lock_config_test_file();
        let temp = TempDir::new().expect("temp dir should be created");
        let game = game();
        let config = config_for(&temp.path().join("backup"), &game);
        let _guard = ConfigFileGuard::write(&config);
        let session = session();
        write_conflict_state(&session, &game);
        let before = read_game_state(&game);

        let result = test_runtime()
            .block_on(resolve_game_conflict(
                &session,
                &memory_operator(),
                &game,
                ConflictResolution::Cancelled,
            ))
            .unwrap();

        assert_eq!(result, ConflictResolutionOutcome::Cancelled);
        let after = read_game_state(&game);
        assert_eq!(before.last_sync_result, after.last_sync_result);
        assert_eq!(before.pending_action, after.pending_action);
    }

    #[test]
    fn resolution_requires_unresolved_conflict_state() {
        let _lock = crate::config::lock_config_test_file();
        let temp = TempDir::new().expect("temp dir should be created");
        let game = game();
        let backup_root = temp.path().join("backup");
        write_local_snapshots(&backup_root, &game, &["base", "local"]);
        let config = config_for(&backup_root, &game);
        let _guard = ConfigFileGuard::write(&config);
        let session = session();
        let op = memory_operator();

        let result = test_runtime().block_on(resolve_game_conflict(
            &session,
            &op,
            &game,
            ConflictResolution::KeepLocal,
        ));

        assert!(matches!(result, Err(BackendError::InvalidConflictState(_))));
    }

    #[test]
    fn keep_local_uploads_metadata_and_clears_conflict_state() {
        let _lock = crate::config::lock_config_test_file();
        let temp = TempDir::new().expect("temp dir should be created");
        let game = game();
        let backup_root = temp.path().join("backup");
        let local = write_local_snapshots(&backup_root, &game, &["base", "local"]);
        let config = config_for(&backup_root, &game);
        let _guard = ConfigFileGuard::write(&config);
        let session = session();
        let op = memory_operator();
        let remote = snapshots(
            &game.name,
            vec![
                snapshot("base", None, String::new()),
                snapshot("remote", Some("base"), String::new()),
            ],
            "remote",
        );
        test_runtime().block_on(write_remote_snapshots(&op, "test-game", &remote));
        write_conflict_state(&session, &game);

        let result = test_runtime()
            .block_on(resolve_game_conflict(
                &session,
                &op,
                &game,
                ConflictResolution::KeepLocal,
            ))
            .unwrap();

        assert_eq!(result, ConflictResolutionOutcome::KeptLocal);
        let remote_path = super::super::utils::game_cloud_metadata_path("test-game").unwrap();
        let remote: GameSnapshots = serde_json::from_slice(
            &test_runtime()
                .block_on(op.read(&remote_path))
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert_eq!(remote.current_device_head(), local.current_device_head());
        let state = read_game_state(&game);
        assert_eq!(state.last_sync_result, Some(SyncResult::Success));
        assert_eq!(state.pending_action, PendingAction::None);
    }

    #[test]
    fn accept_remote_stages_remote_data_and_clears_conflict_state() {
        let _lock = crate::config::lock_config_test_file();
        let temp = TempDir::new().expect("temp dir should be created");
        let game = game();
        let backup_root = temp.path().join("backup");
        write_local_snapshots(&backup_root, &game, &["base", "local"]);
        let config = config_for(&backup_root, &game);
        let _guard = ConfigFileGuard::write(&config);
        let session = session();
        let op = memory_operator();
        let remote = snapshots(
            &game.name,
            vec![
                snapshot("base", None, String::new()),
                snapshot("remote", Some("base"), String::new()),
            ],
            "remote",
        );
        test_runtime().block_on(write_remote_snapshots(&op, "test-game", &remote));
        write_conflict_state(&session, &game);

        let result = test_runtime()
            .block_on(resolve_game_conflict(
                &session,
                &op,
                &game,
                ConflictResolution::AcceptRemote,
            ))
            .unwrap();

        assert_eq!(result, ConflictResolutionOutcome::AcceptedRemote);
        let local: GameSnapshots = serde_json::from_slice(
            &fs::read(backup_root.join("test-game").join("Backups.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            local.current_device_head().map(String::as_str),
            Some("remote")
        );
        assert!(
            local
                .backups
                .iter()
                .all(|snapshot| PathBuf::from(&snapshot.path).starts_with(&backup_root))
        );
        assert_eq!(
            fs::read_to_string(backup_root.join("test-game").join("remote.zip")).unwrap(),
            "remote remote"
        );
        let state = read_game_state(&game);
        assert_eq!(state.last_sync_result, Some(SyncResult::Success));
        assert_eq!(state.pending_action, PendingAction::None);
    }

    #[test]
    fn failed_resolution_keeps_conflict_visible_and_records_error() {
        let _lock = crate::config::lock_config_test_file();
        let temp = TempDir::new().expect("temp dir should be created");
        let game = game();
        let backup_root = temp.path().join("backup");
        write_local_snapshots(&backup_root, &game, &["base", "local"]);
        let config = config_for(&backup_root, &game);
        let _guard = ConfigFileGuard::write(&config);
        let session = session();
        let op = memory_operator();
        let remote = snapshots(
            &game.name,
            vec![
                snapshot("base", None, String::new()),
                snapshot("remote", Some("base"), String::new()),
            ],
            "remote",
        );
        test_runtime().block_on(write_remote_metadata(&op, "test-game", &remote));
        write_conflict_state(&session, &game);

        let result = test_runtime().block_on(resolve_game_conflict(
            &session,
            &op,
            &game,
            ConflictResolution::AcceptRemote,
        ));

        assert!(result.is_err());
        let state = read_game_state(&game);
        assert!(matches!(state.last_sync_result, Some(SyncResult::Error(_))));
        assert_eq!(state.last_known_local_head.as_deref(), Some("local"));
        assert_eq!(state.last_known_remote_head.as_deref(), Some("remote"));
        assert_eq!(state.pending_action, PendingAction::UserDecisionRequired);
    }
}
