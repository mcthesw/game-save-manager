use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::path_pattern::{ManifestPathConstraints, ManifestPathPattern, PlatformKind, StoreKind};

pub type ResourceId = String;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlatformPaths {
    pub home: Option<PathBuf>,
    pub os_user_name: Option<String>,
    pub win_app_data: Option<PathBuf>,
    pub win_local_app_data: Option<PathBuf>,
    pub win_local_app_data_low: Option<PathBuf>,
    pub win_documents: Option<PathBuf>,
    pub win_public: Option<PathBuf>,
    pub win_program_data: Option<PathBuf>,
    pub win_dir: Option<PathBuf>,
    pub xdg_data: Option<PathBuf>,
    pub xdg_config: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameRootCandidate {
    pub id: ResourceId,
    pub store: StoreKind,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreAccountCandidate {
    pub id: ResourceId,
    pub store: StoreKind,
    pub user_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameInstallationCandidate {
    pub id: ResourceId,
    pub root_id: ResourceId,
    pub store: StoreKind,
    pub install_dir: String,
    pub install_path: PathBuf,
    pub store_game_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolutionSelection {
    pub root_ids: Option<BTreeSet<ResourceId>>,
    pub account_ids: Option<BTreeSet<ResourceId>>,
    pub installation_ids: Option<BTreeSet<ResourceId>>,
}

impl ResolutionSelection {
    pub fn is_explicit(&self) -> bool {
        self.root_ids.is_some() || self.account_ids.is_some() || self.installation_ids.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct ResolutionContext {
    pub platform: PlatformKind,
    pub platform_paths: PlatformPaths,
    pub roots: Vec<GameRootCandidate>,
    pub accounts: Vec<StoreAccountCandidate>,
    pub installations: Vec<GameInstallationCandidate>,
    pub store_game_ids: BTreeMap<StoreKind, String>,
    pub selection: ResolutionSelection,
}

impl Default for ResolutionContext {
    fn default() -> Self {
        Self {
            platform: PlatformKind::host(),
            platform_paths: PlatformPaths::default(),
            roots: Vec::new(),
            accounts: Vec::new(),
            installations: Vec::new(),
            store_game_ids: BTreeMap::new(),
            selection: ResolutionSelection::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, Default)]
#[serde(rename_all = "camelCase")]
pub struct CandidateDimensions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_id: Option<ResourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<ResourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installation_id: Option<ResourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<StoreKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CandidateExpression {
    pub id: String,
    pub expression: String,
    pub logical_anchor: String,
    pub dimensions: CandidateDimensions,
    pub case_sensitive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ResolutionSelectionState {
    Missing,
    ImplicitUnique {
        candidate_id: String,
    },
    Ambiguous {
        candidate_ids: Vec<String>,
    },
    Explicit {
        candidate_ids: Vec<String>,
    },
    StaleSelection {
        selected_resource_ids: Vec<ResourceId>,
        candidate_ids: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ResolutionDiagnosticKind {
    UnknownPlaceholder,
    InvalidGlob,
    MissingContext,
    UnsupportedPlatform,
    NoCandidate,
    NoMatch,
    MultipleCandidates,
    MultipleMatches,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolutionDiagnostic {
    pub kind: ResolutionDiagnosticKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionPlan {
    pub pattern: ManifestPathPattern,
    pub constraints: ManifestPathConstraints,
    pub candidates: Vec<CandidateExpression>,
    pub selection_state: ResolutionSelectionState,
    pub diagnostics: Vec<ResolutionDiagnostic>,
}

impl ResolutionPlan {
    pub fn is_blocked(&self) -> bool {
        matches!(
            self.selection_state,
            ResolutionSelectionState::Missing
                | ResolutionSelectionState::Ambiguous { .. }
                | ResolutionSelectionState::StaleSelection { .. }
        ) || self.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.kind,
                ResolutionDiagnosticKind::UnknownPlaceholder
                    | ResolutionDiagnosticKind::InvalidGlob
                    | ResolutionDiagnosticKind::MissingContext
                    | ResolutionDiagnosticKind::UnsupportedPlatform
                    | ResolutionDiagnosticKind::NoCandidate
            )
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ResolvedLocationKind {
    File,
    Directory,
    Registry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSaveLocation {
    pub path: String,
    pub kind: ResolvedLocationKind,
    pub candidate_id: String,
    pub logical_anchor: String,
    pub dimensions: CandidateDimensions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ResolutionReport {
    pub raw_pattern: String,
    pub selection_state: ResolutionSelectionState,
    pub candidates: Vec<CandidateExpression>,
    pub locations: Vec<ResolvedSaveLocation>,
    pub diagnostics: Vec<ResolutionDiagnostic>,
}
