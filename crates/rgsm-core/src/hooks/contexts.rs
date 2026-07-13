use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::backup::{Game, GameSnapshots, Snapshot};
use crate::config::Config;

/// Describes *why* the lifecycle event was triggered so downstream hooks can
/// adjust behaviour (for example, only playing sounds for quick actions).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum HookSource {
    UserManual,
    TimerAutoBackup,
    BatchOperation,
    QuickActionHotkey,
    QuickActionTray,
    ProcessMonitorAutoBackup,
    CloudSync,
    Internal,
}

pub struct SnapshotCreatedCtx {
    pub config: Config,
    pub source: HookSource,
    pub game: Game,
    pub snapshot: Snapshot,
    pub snapshots: GameSnapshots,
    pub local_archive_path: PathBuf,
    pub remote_archive_path: String,
}

pub struct SnapshotDeletedCtx {
    pub config: Config,
    pub source: HookSource,
    pub game: Game,
    pub snapshots: GameSnapshots,
    pub deleted_remote_paths: Vec<String>,
}

#[allow(dead_code)]
pub struct SnapshotAppliedCtx {
    pub config: Config,
    pub source: HookSource,
    pub game: Game,
    pub snapshot: Snapshot,
    pub snapshots: GameSnapshots,
}

#[allow(dead_code)]
pub struct BeforeRestoreCtx {
    pub config: Config,
    pub source: HookSource,
    pub game: Game,
    pub snapshot: Snapshot,
    pub snapshots: GameSnapshots,
    pub archive_path: PathBuf,
    pub capture_plan: Option<crate::backup::CapturePlan>,
}

pub struct MetadataChangedCtx {
    pub config: Config,
    pub source: HookSource,
    pub game: Game,
    pub snapshots: GameSnapshots,
}

pub struct GameAddedCtx {
    pub config: Config,
    pub source: HookSource,
    pub game: Game,
    pub snapshots: GameSnapshots,
}

#[allow(dead_code)]
pub struct GameUpdatedCtx {
    pub config: Config,
    pub source: HookSource,
    pub previous_game: Game,
    pub game: Game,
}

pub struct GameDeletedCtx {
    pub config: Config,
    pub source: HookSource,
    pub game_name: String,
    pub remote_game_dir_path: String,
}

#[allow(dead_code)]
pub struct ConfigSavedCtx {
    pub config: Config,
    pub source: HookSource,
}

#[allow(dead_code)]
pub struct SyncCompletedCtx {
    pub config: Config,
    pub source: HookSource,
    pub game_name: String,
    pub success: bool,
    pub message: Option<String>,
}

#[allow(dead_code)]
pub struct SyncConflictCtx {
    pub config: Config,
    pub source: HookSource,
    pub game_name: String,
    pub local_head: Option<String>,
    pub remote_head: Option<String>,
}
