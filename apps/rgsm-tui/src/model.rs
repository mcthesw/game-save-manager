use rgsm_core::backup::{Game, GameSnapshots};
use rgsm_core::cloud_sync::SyncState;
use rgsm_core::config::Config;
use rgsm_core::ludusavi_manifest::{ImportableGame, LudusaviManifestStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    GameEditor,
    Ludusavi,
    Cloud,
    Settings,
    Logs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsItem {
    AutoCloudEnqueue,
    LudusaviLocalOnly,
    ImportGuiProfile,
    CurrentDeviceName,
    AddGameRoot,
    AddVnScanRoot,
}

impl SettingsItem {
    pub const ALL: [Self; 6] = [
        Self::AutoCloudEnqueue,
        Self::LudusaviLocalOnly,
        Self::ImportGuiProfile,
        Self::CurrentDeviceName,
        Self::AddGameRoot,
        Self::AddVnScanRoot,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalKind {
    Help,
    Confirm,
    Prompt,
    Message,
}

#[derive(Debug, Clone)]
pub struct Modal {
    pub kind: ModalKind,
    pub title: String,
    pub message: String,
    pub input: String,
    pub action: PendingAction,
}

#[derive(Debug, Clone)]
pub enum PendingAction {
    None,
    AcknowledgeExperimentalWarning,
    AddGameName,
    RenameGame,
    AddSaveUnitPath,
    EditSelectedPath,
    CreateSnapshot,
    CreateSnapshotFromSelected,
    RestoreSnapshot,
    DeleteSnapshot,
    BatchDeleteSnapshots,
    EditSnapshotDescription,
    SetCurrentPosition,
    DetachSnapshot,
    DeleteGame,
    CheckCloud,
    UploadAll,
    DownloadAll,
    SyncSelected,
    SyncAll,
    EditCloudSettings,
    ResolveKeepLocal,
    ResolveUseCloud,
    ImportSelectedGame,
    SearchGames,
    SearchImportableGames,
    EditImportPath,
    ImportGuiProfile,
    EditCurrentDeviceName,
    AddCurrentDeviceRoot,
    AddVnScanRoot,
    ImportVnScanResults,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListSort {
    Natural,
    NameAsc,
    NameDesc,
}

impl ListSort {
    pub fn next(self) -> Self {
        match self {
            Self::Natural => Self::NameAsc,
            Self::NameAsc => Self::NameDesc,
            Self::NameDesc => Self::Natural,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportCandidate {
    pub game: ImportableGame,
    pub save_paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AppData {
    pub config: Config,
    pub games: Vec<Game>,
    pub selected_snapshots: Option<GameSnapshots>,
    pub sync_state: SyncState,
    pub manifest_status: LudusaviManifestStatus,
    pub importable_games: Vec<ImportCandidate>,
}

#[derive(Debug, Clone, Default)]
pub struct Selection {
    pub game: usize,
    pub snapshot: usize,
    pub save_unit: usize,
    pub device: usize,
    pub importable: usize,
    pub import_path: usize,
    pub settings: usize,
    pub log_scroll: u16,
}

#[derive(Debug, Clone)]
pub enum OperationEvent {
    Started(String),
    Finished { status: String, detail: String },
    Failed(String),
    DataReloaded(Box<AppData>),
}
