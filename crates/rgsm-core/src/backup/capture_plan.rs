use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

use crate::path_resolution::{
    CandidateDimensions, ResolutionDiagnostic, ResolutionReport, ResolutionSelectionState,
    ResolvedLocationKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum CaptureSourceKind {
    File,
    Directory,
    Registry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureGroup {
    pub id: u32,
    pub save_unit_id: u32,
    pub candidate_id: String,
    pub dimensions: CandidateDimensions,
    pub logical_anchor: PathBuf,
    pub source_path: String,
    pub relative_path: String,
    pub archive_path: String,
    pub kind: CaptureSourceKind,
    pub delete_before_apply: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CapturePlan {
    pub groups: Vec<CaptureGroup>,
}

#[derive(Debug, Clone)]
pub struct SaveUnitCaptureInput {
    pub save_unit_id: u32,
    pub delete_before_apply: bool,
    pub report: ResolutionReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CapturePreflightFailure {
    pub save_unit_id: u32,
    pub selection_state: ResolutionSelectionState,
    pub diagnostics: Vec<ResolutionDiagnostic>,
}

#[derive(Debug, Error)]
pub enum CapturePlanError {
    #[error("capture preflight failed for {count} save unit(s)", count = .0.len())]
    Blocking(Vec<CapturePreflightFailure>),
    #[error("no enabled save data currently matches")]
    NoDataMatched,
}

impl CapturePlan {
    pub fn from_resolution_reports(
        inputs: Vec<SaveUnitCaptureInput>,
    ) -> Result<Self, CapturePlanError> {
        let failures = inputs
            .iter()
            .filter(|input| !is_non_applicable_report(&input.report))
            .filter(|input| is_blocking_report(&input.report))
            .map(|input| CapturePreflightFailure {
                save_unit_id: input.save_unit_id,
                selection_state: input.report.selection_state.clone(),
                diagnostics: input.report.diagnostics.clone(),
            })
            .collect::<Vec<_>>();
        if !failures.is_empty() {
            return Err(CapturePlanError::Blocking(failures));
        }

        let mut groups = Vec::new();
        for input in inputs {
            for location in input.report.locations {
                let source_path = PathBuf::from(&location.path);
                let logical_anchor = PathBuf::from(&location.logical_anchor);
                let relative_path =
                    relative_path(&logical_anchor, &source_path).unwrap_or_else(|| {
                        source_path
                            .file_name()
                            .map(PathBuf::from)
                            .unwrap_or_default()
                    });
                let kind = match location.kind {
                    ResolvedLocationKind::File => CaptureSourceKind::File,
                    ResolvedLocationKind::Directory => CaptureSourceKind::Directory,
                    ResolvedLocationKind::Registry => CaptureSourceKind::Registry,
                };
                let id = groups.len() as u32;
                let archive_path = archive_path(input.save_unit_id, id, &source_path, kind);
                groups.push(CaptureGroup {
                    id,
                    save_unit_id: input.save_unit_id,
                    candidate_id: location.candidate_id,
                    dimensions: location.dimensions,
                    logical_anchor,
                    source_path: location.path,
                    relative_path: render_relative_path(&relative_path),
                    archive_path,
                    kind,
                    delete_before_apply: input.delete_before_apply,
                });
            }
        }
        if groups.is_empty() {
            return Err(CapturePlanError::NoDataMatched);
        }
        Ok(Self { groups })
    }
}

fn is_non_applicable_report(report: &ResolutionReport) -> bool {
    report.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == crate::path_resolution::ResolutionDiagnosticKind::UnsupportedPlatform
    })
}

fn is_blocking_report(report: &ResolutionReport) -> bool {
    matches!(
        report.selection_state,
        ResolutionSelectionState::Missing
            | ResolutionSelectionState::Ambiguous { .. }
            | ResolutionSelectionState::StaleSelection { .. }
    ) || report.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.kind,
            crate::path_resolution::ResolutionDiagnosticKind::UnknownPlaceholder
                | crate::path_resolution::ResolutionDiagnosticKind::InvalidGlob
                | crate::path_resolution::ResolutionDiagnosticKind::MissingContext
                | crate::path_resolution::ResolutionDiagnosticKind::UnsupportedPlatform
                | crate::path_resolution::ResolutionDiagnosticKind::NoCandidate
        )
    })
}

fn archive_path(
    save_unit_id: u32,
    group_id: u32,
    source_path: &Path,
    kind: CaptureSourceKind,
) -> String {
    let name = match kind {
        CaptureSourceKind::Registry => "registry.reg".into(),
        CaptureSourceKind::File | CaptureSourceKind::Directory => source_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
    };
    format!("{save_unit_id}/{group_id}/data/{name}")
}

fn relative_path(anchor: &Path, target: &Path) -> Option<PathBuf> {
    let anchor = anchor.components().collect::<Vec<_>>();
    let target = target.components().collect::<Vec<_>>();
    let mut shared = 0;
    while shared < anchor.len()
        && shared < target.len()
        && components_equal(anchor[shared], target[shared])
    {
        shared += 1;
    }
    if shared == 0 && (!anchor.is_empty() || !target.is_empty()) {
        return None;
    }

    let mut relative = PathBuf::new();
    for component in &anchor[shared..] {
        if matches!(component, Component::Normal(_)) {
            relative.push("..");
        }
    }
    for component in &target[shared..] {
        relative.push(component.as_os_str());
    }
    Some(relative)
}

fn components_equal(left: Component<'_>, right: Component<'_>) -> bool {
    if cfg!(target_os = "windows") {
        left.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.as_os_str().to_string_lossy())
    } else {
        left == right
    }
}

fn render_relative_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path_resolution::{CandidateExpression, ResolutionSelectionState};

    fn report(paths: &[(&str, ResolvedLocationKind)]) -> ResolutionReport {
        ResolutionReport {
            raw_pattern: "<home>/**/*.sav".to_string(),
            selection_state: ResolutionSelectionState::ImplicitUnique {
                candidate_id: "platform".to_string(),
            },
            candidates: vec![CandidateExpression {
                id: "platform".to_string(),
                expression: "C:/Users/Player/**/*.sav".to_string(),
                logical_anchor: "C:/Users/Player".to_string(),
                dimensions: CandidateDimensions::default(),
                case_sensitive: false,
            }],
            locations: paths
                .iter()
                .map(
                    |(path, kind)| crate::path_resolution::ResolvedSaveLocation {
                        path: (*path).to_string(),
                        kind: *kind,
                        candidate_id: "platform".to_string(),
                        logical_anchor: "C:/Users/Player".to_string(),
                        dimensions: CandidateDimensions::default(),
                    },
                )
                .collect(),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn builds_deterministic_groups_for_every_match() {
        let plan = CapturePlan::from_resolution_reports(vec![SaveUnitCaptureInput {
            save_unit_id: 4,
            delete_before_apply: true,
            report: report(&[
                ("C:/Users/Player/Saves/a.sav", ResolvedLocationKind::File),
                ("C:/Users/Player/Saves/b.sav", ResolvedLocationKind::File),
            ]),
        }])
        .unwrap();

        assert_eq!(plan.groups.len(), 2);
        assert_eq!(plan.groups[0].relative_path, "Saves/a.sav");
        assert_eq!(plan.groups[0].archive_path, "4/0/data/a.sav");
        assert_eq!(plan.groups[1].archive_path, "4/1/data/b.sav");
        assert!(plan.groups.iter().all(|group| group.delete_before_apply));
    }

    #[test]
    fn aggregates_blocking_save_units_before_capture() {
        let mut first = report(&[]);
        first.selection_state = ResolutionSelectionState::Ambiguous {
            candidate_ids: vec!["a".to_string(), "b".to_string()],
        };
        let mut second = report(&[]);
        second.selection_state = ResolutionSelectionState::Missing;

        let error = CapturePlan::from_resolution_reports(vec![
            SaveUnitCaptureInput {
                save_unit_id: 1,
                delete_before_apply: false,
                report: first,
            },
            SaveUnitCaptureInput {
                save_unit_id: 2,
                delete_before_apply: false,
                report: second,
            },
        ])
        .unwrap_err();

        let CapturePlanError::Blocking(failures) = error else {
            panic!("expected blocking preflight");
        };
        assert_eq!(failures.len(), 2);
    }

    #[test]
    fn blocking_diagnostic_prevents_capture_even_with_explicit_selection() {
        let mut invalid = report(&[]);
        invalid.selection_state = ResolutionSelectionState::Explicit {
            candidate_ids: vec!["platform".to_string()],
        };
        invalid.diagnostics.push(ResolutionDiagnostic {
            kind: crate::path_resolution::ResolutionDiagnosticKind::InvalidGlob,
            message: "invalid glob".to_string(),
        });

        let error = CapturePlan::from_resolution_reports(vec![SaveUnitCaptureInput {
            save_unit_id: 7,
            delete_before_apply: false,
            report: invalid,
        }])
        .unwrap_err();

        assert!(
            matches!(error, CapturePlanError::Blocking(failures) if failures[0].save_unit_id == 7)
        );
    }

    #[test]
    fn unsupported_platform_reports_are_skipped_when_other_units_match() {
        let mut unsupported = report(&[]);
        unsupported.selection_state = ResolutionSelectionState::Missing;
        unsupported.diagnostics.push(ResolutionDiagnostic {
            kind: crate::path_resolution::ResolutionDiagnosticKind::UnsupportedPlatform,
            message: "not applicable on this platform".to_string(),
        });

        let plan = CapturePlan::from_resolution_reports(vec![
            SaveUnitCaptureInput {
                save_unit_id: 1,
                delete_before_apply: false,
                report: report(&[("C:/Users/Player/Saves/game.sav", ResolvedLocationKind::File)]),
            },
            SaveUnitCaptureInput {
                save_unit_id: 2,
                delete_before_apply: false,
                report: unsupported,
            },
        ])
        .unwrap();

        assert_eq!(plan.groups.len(), 1);
        assert_eq!(plan.groups[0].save_unit_id, 1);
    }

    #[test]
    fn only_unsupported_platform_reports_return_no_data() {
        let mut unsupported = report(&[]);
        unsupported.selection_state = ResolutionSelectionState::Missing;
        unsupported.diagnostics.push(ResolutionDiagnostic {
            kind: crate::path_resolution::ResolutionDiagnosticKind::UnsupportedPlatform,
            message: "not applicable on this platform".to_string(),
        });

        assert!(matches!(
            CapturePlan::from_resolution_reports(vec![SaveUnitCaptureInput {
                save_unit_id: 2,
                delete_before_apply: false,
                report: unsupported,
            }]),
            Err(CapturePlanError::NoDataMatched)
        ));
    }

    #[test]
    fn all_zero_match_reports_return_no_data() {
        let error = CapturePlan::from_resolution_reports(vec![SaveUnitCaptureInput {
            save_unit_id: 1,
            delete_before_apply: false,
            report: report(&[]),
        }])
        .unwrap_err();

        assert!(matches!(error, CapturePlanError::NoDataMatched));
    }
}
