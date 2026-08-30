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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema, Default)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CandidateExpression {
    pub id: String,
    pub expression: String,
    pub logical_anchor: String,
    pub dimensions: CandidateDimensions,
    pub case_sensitive: bool,
}

impl CandidateExpression {
    pub fn is_exact(&self) -> bool {
        first_unescaped_glob(&self.expression).is_none()
    }

    /// Return the concrete path represented by an exact candidate expression.
    /// Glob escaping is an expression-level concern and must not leak into the
    /// filesystem target used by restore operations.
    pub fn exact_target_path(&self) -> Option<PathBuf> {
        self.is_exact()
            .then(|| PathBuf::from(unescape_glob_literal(&self.expression)))
    }
}

pub(super) fn unescape_glob_literal(value: &str) -> String {
    value
        .replace("[[]", "[")
        .replace("[]]", "]")
        .replace("[*]", "*")
        .replace("[?]", "?")
}

pub(super) fn first_unescaped_glob(expression: &str) -> Option<usize> {
    let bytes = expression.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"[[]")
            || bytes[index..].starts_with(b"[]]")
            || bytes[index..].starts_with(b"[*]")
            || bytes[index..].starts_with(b"[?]")
        {
            index += 3;
            continue;
        }
        if matches!(bytes[index], b'*' | b'?' | b'[') {
            return Some(index);
        }
        index += 1;
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ResolvedLocationKind {
    File,
    Directory,
    Registry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSaveLocation {
    pub path: String,
    pub kind: ResolvedLocationKind,
    pub candidate_id: String,
    pub logical_anchor: String,
    pub dimensions: CandidateDimensions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolutionReport {
    pub raw_pattern: String,
    pub selection_state: ResolutionSelectionState,
    pub candidates: Vec<CandidateExpression>,
    pub locations: Vec<ResolvedSaveLocation>,
    pub diagnostics: Vec<ResolutionDiagnostic>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(expression: &str, logical_anchor: &str) -> CandidateExpression {
        CandidateExpression {
            id: "candidate".to_string(),
            expression: expression.to_string(),
            logical_anchor: logical_anchor.to_string(),
            dimensions: CandidateDimensions::default(),
            case_sensitive: false,
        }
    }

    #[test]
    fn exactness_depends_on_unescaped_globs_not_the_logical_anchor() {
        assert!(candidate("C:/Users/Player/Saved", "C:/Users/Player").is_exact());
        assert!(!candidate("C:/Users/Player/*.sav", "C:/Users/Player").is_exact());
    }

    #[test]
    fn exact_target_decodes_escaped_glob_literals() {
        let exact = candidate("C:/Games[[]Main[]]/slot[*]-answer[?].sav", "C:/Games[Main]");

        assert_eq!(
            exact.exact_target_path(),
            Some(PathBuf::from("C:/Games[Main]/slot*-answer?.sav"))
        );
        assert_eq!(
            candidate("C:/Games/*.sav", "C:/Games").exact_target_path(),
            None
        );
    }

    #[test]
    fn escaped_glob_characters_are_exact_path_literals() {
        assert!(candidate("D:/Games[[]Main[]]/slot[*].sav", "D:/Games[Main]").is_exact());
        assert!(!candidate("D:/Games[[]Main[]]/slot[0-9].sav", "D:/Games[Main]").is_exact());
    }
}
