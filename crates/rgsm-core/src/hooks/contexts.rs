use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::backup::{Game, GameSnapshots, Snapshot};
use crate::config::Config;

/// Describes *why* the lifecycle event was triggered so downstream hooks can
/// adjust behaviour (for example, only playing sounds for quick actions).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HookSource {
    UserManual,
    TimerAutoBackup,
    BatchOperation,
    QuickActionHotkey,
    QuickActionTray,
    ProcessMonitorAutoBackup,
    CloudSync,
    CloudConflictResolution,
    Internal,
}

pub struct SnapshotCreatedCtx {
    pub config: Config,
    pub source: HookSource,
    pub game: Game,
    pub snapshot: Snapshot,
    pub snapshots: GameSnapshots,
    pub local_archive_path: PathBuf,
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
    /// Current save data for the pre-restore safety backup. `None` means there
    /// is nothing to back up (or the backup is disabled); a plan that cannot be
    /// built aborts the restore before any hook runs.
    pub capture_plan: Option<crate::backup::CapturePlan>,
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
    pub game: Game,
}

pub struct GameDeletedCtx {
    pub config: Config,
    pub source: HookSource,
    pub game_name: String,
}

#[allow(dead_code)]
pub struct ConfigSavedCtx {
    pub config: Config,
    pub source: HookSource,
}
