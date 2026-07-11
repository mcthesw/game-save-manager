use std::collections::BTreeSet;

use thiserror::Error;

use super::{
    ResolutionDiagnostic, ResolutionDiagnosticKind, ResolutionPlan, ResolutionReport,
    ResolvedLocationKind, ResolvedSaveLocation,
};

#[derive(Debug, Error)]
pub enum MatchError {
    #[error("invalid glob expression {expression}: {message}")]
    InvalidGlob { expression: String, message: String },
    #[error("failed to inspect matched path {path}: {source}")]
    Metadata {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

pub fn match_resolution_plan(plan: &ResolutionPlan) -> Result<ResolutionReport, MatchError> {
    let mut diagnostics = plan.diagnostics.clone();
    let mut locations = Vec::new();
    let mut seen = BTreeSet::new();

    if !plan.is_blocked() {
        for candidate in &plan.candidates {
            let options = globetter::MatchOptions {
                case_sensitive: candidate.case_sensitive,
                require_literal_separator: true,
                require_literal_leading_dot: false,
                follow_links: true,
            };
            let paths = globetter::glob_with(&candidate.expression, options).map_err(|error| {
                MatchError::InvalidGlob {
                    expression: candidate.expression.clone(),
                    message: error.to_string(),
                }
            })?;

            for result in paths {
                let path = result.map_err(|error| MatchError::InvalidGlob {
                    expression: candidate.expression.clone(),
                    message: error.to_string(),
                })?;
                let metadata = path.metadata().map_err(|source| MatchError::Metadata {
                    path: path.to_string_lossy().into_owned(),
                    source,
                })?;
                let kind = if metadata.is_file() {
                    ResolvedLocationKind::File
                } else if metadata.is_dir() {
                    ResolvedLocationKind::Directory
                } else {
                    continue;
                };
                let rendered = path.to_string_lossy().into_owned();
                let identity = if candidate.case_sensitive {
                    rendered.clone()
                } else {
                    rendered.to_lowercase()
                };
                if seen.insert(identity) {
                    locations.push(ResolvedSaveLocation {
                        path: rendered,
                        kind,
                        candidate_id: candidate.id.clone(),
                        logical_anchor: candidate.logical_anchor.clone(),
                        dimensions: candidate.dimensions.clone(),
                    });
                }
            }
        }
    }

    locations.sort_by(|left, right| {
        left.path
            .to_lowercase()
            .cmp(&right.path.to_lowercase())
            .then(left.path.cmp(&right.path))
    });
    if locations.is_empty() && !plan.is_blocked() {
        diagnostics.push(ResolutionDiagnostic {
            kind: ResolutionDiagnosticKind::NoMatch,
            message: "the valid path pattern has no current filesystem matches".to_string(),
        });
    } else if locations.len() > 1 {
        diagnostics.push(ResolutionDiagnostic {
            kind: ResolutionDiagnosticKind::MultipleMatches,
            message: format!(
                "the path pattern currently matches {} locations",
                locations.len()
            ),
        });
    }

    Ok(ResolutionReport {
        raw_pattern: plan.pattern.raw().to_string(),
        selection_state: plan.selection_state.clone(),
        candidates: plan.candidates.clone(),
        locations,
        diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::path_pattern::{ManifestPathConstraints, parse_manifest_path_pattern};
    use crate::path_resolution::{
        PlatformPaths, ResolutionContext, ResolutionSelectionState, plan_resolution,
    };

    #[test]
    fn matches_files_recursively_and_in_stable_order() {
        let temp = temp_dir::TempDir::new().unwrap();
        let root = temp.path().join("Save[Main]");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("b.sav"), b"b").unwrap();
        fs::write(root.join("nested").join("a.sav"), b"a").unwrap();
        fs::write(root.join("ignored.txt"), b"x").unwrap();

        let parsed = parse_manifest_path_pattern(format!(
            "{}/**/*.sav",
            globset::escape(&root.to_string_lossy().replace('\\', "/"))
        ))
        .unwrap();
        let context = ResolutionContext {
            platform_paths: PlatformPaths {
                home: Some(temp.path().to_path_buf()),
                ..PlatformPaths::default()
            },
            ..ResolutionContext::default()
        };
        let plan = plan_resolution(&parsed, ManifestPathConstraints::default(), &context);

        assert!(matches!(
            plan.selection_state,
            ResolutionSelectionState::ImplicitUnique { .. }
        ));
        let report = match_resolution_plan(&plan).unwrap();

        assert_eq!(report.locations.len(), 2);
        assert!(
            report
                .locations
                .iter()
                .any(|location| location.path.ends_with("a.sav"))
        );
        assert!(
            report
                .locations
                .iter()
                .any(|location| location.path.ends_with("b.sav"))
        );
        assert!(
            report
                .locations
                .windows(2)
                .all(|pair| { pair[0].path.to_lowercase() <= pair[1].path.to_lowercase() })
        );
    }

    #[test]
    fn zero_matches_is_a_non_blocking_diagnostic() {
        let temp = temp_dir::TempDir::new().unwrap();
        let parsed = parse_manifest_path_pattern(format!(
            "{}/*.sav",
            temp.path().to_string_lossy().replace('\\', "/")
        ))
        .unwrap();
        let plan = plan_resolution(
            &parsed,
            ManifestPathConstraints::default(),
            &ResolutionContext::default(),
        );

        assert!(!plan.is_blocked());
        let report = match_resolution_plan(&plan).unwrap();

        assert!(report.locations.is_empty());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.kind == ResolutionDiagnosticKind::NoMatch)
        );
    }

    #[test]
    fn reevaluating_a_pattern_includes_files_created_after_import() {
        let temp = temp_dir::TempDir::new().unwrap();
        let parsed = parse_manifest_path_pattern(format!(
            "{}/*.sav",
            temp.path().to_string_lossy().replace('\\', "/")
        ))
        .unwrap();
        let plan = plan_resolution(
            &parsed,
            ManifestPathConstraints::default(),
            &ResolutionContext::default(),
        );
        assert!(match_resolution_plan(&plan).unwrap().locations.is_empty());

        fs::write(temp.path().join("created-later.sav"), b"save").unwrap();

        let report = match_resolution_plan(&plan).unwrap();
        assert_eq!(report.locations.len(), 1);
        assert!(report.locations[0].path.ends_with("created-later.sav"));
    }
}
