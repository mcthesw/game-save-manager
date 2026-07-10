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
