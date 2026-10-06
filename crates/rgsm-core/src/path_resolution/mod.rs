pub(crate) mod context;
mod matcher;
mod model;
mod planner;

pub use matcher::{MatchError, match_resolution_plan};
pub use model::{
    CandidateDimensions, CandidateExpression, GameInstallationCandidate, GameRootCandidate,
    PlatformPaths, ResolutionContext, ResolutionDiagnostic, ResolutionDiagnosticKind,
    ResolutionPlan, ResolutionReport, ResolutionSelection, ResolutionSelectionState,
    ResolvedLocationKind, ResolvedSaveLocation, ResourceId, StoreAccountCandidate,
};
pub use planner::plan_resolution;

/// Concrete paths share variable selection with patterns, but treat glob characters literally.
pub fn resolve_single_path(
    raw: &str,
    context: &ResolutionContext,
) -> Result<std::path::PathBuf, String> {
    if !raw.contains('<') && !raw.contains('>') {
        return Ok(raw.into());
    }
    let parsed = crate::path_pattern::parse_manifest_path_pattern(globset::escape(raw))
        .map_err(|_| rust_i18n::t!("path_variable.invalid_pattern").to_string())?;
    let plan = plan_resolution(&parsed, Default::default(), context);
    if plan.is_blocked() || plan.candidates.len() != 1 {
        return Err(selection_error(&plan.selection_state));
    }
    plan.candidates[0]
        .exact_target_path()
        .ok_or_else(|| rust_i18n::t!("path_variable.invalid_pattern").to_string())
}

pub fn selection_error(state: &ResolutionSelectionState) -> String {
    match state {
        ResolutionSelectionState::Ambiguous { .. } | ResolutionSelectionState::Explicit { .. } => {
            rust_i18n::t!("path_variable.choose_location").to_string()
        }
        ResolutionSelectionState::StaleSelection { .. } => {
            rust_i18n::t!("path_variable.selection_unavailable").to_string()
        }
        _ => rust_i18n::t!("path_variable.missing_context").to_string(),
    }
}
