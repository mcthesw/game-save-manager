use std::collections::BTreeMap;
use std::path::PathBuf;

use thiserror::Error;

use crate::path_resolution::ResolutionReport;

use super::CaptureSourceKind;
use super::archive::ArchiveCaptureGroup;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreEntry {
    pub save_unit_id: u32,
    pub group_id: u32,
    pub archive_path: String,
    pub target_path: PathBuf,
    pub kind: CaptureSourceKind,
    pub delete_before_apply: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RestorePlan {
    pub entries: Vec<RestoreEntry>,
    pub skipped_inactive_save_unit_ids: Vec<u32>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RestorePlanError {
    #[error("{0}")]
    VariableExpression(String),
    #[error("{}", rust_i18n::t!("path_variable.override_incompatible", unit = save_unit_id))]
    OverrideIncompatible { save_unit_id: u32 },
    #[error("{}", rust_i18n::t!("path_variable.choose_location"))]
    SelectionRequired { save_unit_id: u32 },
    #[error("{}", rust_i18n::t!("path_variable.multiple_instances"))]
    MultipleSourceInstances { save_unit_id: u32 },
    #[error("{}", rust_i18n::t!("path_variable.restore_overlap", path = path))]
    OverlappingTargets { path: String },
    #[error(
        "legacy archive cannot restore wildcard save location for save unit {save_unit_id}, capture group {group_id}"
    )]
    LegacyWildcardTarget { save_unit_id: u32, group_id: u32 },
    #[error("archive has no active target save units: {save_unit_ids:?}")]
    NoActiveTargetSaveUnits { save_unit_ids: Vec<u32> },
}

impl RestorePlan {
    pub fn build(
        groups: &[ArchiveCaptureGroup],
        reports: &BTreeMap<u32, ResolutionReport>,
    ) -> Result<Self, RestorePlanError> {
        let mut entries = Vec::new();
        let mut skipped = std::collections::BTreeSet::new();
        for group in groups {
            let Some(report) = reports.get(&group.save_unit_id) else {
                skipped.insert(group.save_unit_id);
                continue;
            };
            let candidate = selected_candidate(group, groups, report)?;
            entries.push(RestoreEntry {
                save_unit_id: group.save_unit_id,
                group_id: group.id,
                archive_path: group.archive_path.clone(),
                target_path: if group.kind == CaptureSourceKind::Registry
                    || (candidate.is_exact()
                        && groups
                            .iter()
                            .filter(|source| source.save_unit_id == group.save_unit_id)
                            .count()
                            == 1)
                {
                    candidate
                        .exact_target_path()
                        .ok_or(RestorePlanError::SelectionRequired {
                            save_unit_id: group.save_unit_id,
                        })?
                } else {
                    let relative = if let Some(expression) = &group.relative_expression {
                        let values = candidate
                            .variable_pattern
                            .as_ref()
                            .map(|p| &p.values)
                            .ok_or_else(|| {
                                RestorePlanError::VariableExpression(
                                    "target variable bindings are unavailable".into(),
                                )
                            })?;
                        crate::path_variables::restore_relative_expression(expression, values)
                            .map_err(RestorePlanError::VariableExpression)?
                    } else {
                        PathBuf::from(&group.relative_path)
                    };
                    PathBuf::from(&candidate.logical_anchor).join(relative)
                },
                kind: group.kind,
                delete_before_apply: group.delete_before_apply,
            });
        }
        finish_plan(entries, skipped)
    }

    /// Build targets for Archive V2, whose ID-prefixed entries predate the
    /// embedded capture manifest. Exact current candidates are authoritative;
    /// wildcard patterns are rejected because V2 does not record which match
    /// produced the archived save unit.
    pub fn build_legacy_v2(
        groups: &[ArchiveCaptureGroup],
        reports: &BTreeMap<u32, ResolutionReport>,
    ) -> Result<Self, RestorePlanError> {
        let mut entries = Vec::new();
        let mut skipped = std::collections::BTreeSet::new();
        for group in groups {
            let Some(report) = reports.get(&group.save_unit_id) else {
                skipped.insert(group.save_unit_id);
                continue;
            };
            let candidate = selected_candidate(group, groups, report)?;
            let Some(target_path) = candidate.exact_target_path() else {
                return Err(RestorePlanError::LegacyWildcardTarget {
                    save_unit_id: group.save_unit_id,
                    group_id: group.id,
                });
            };
            entries.push(RestoreEntry {
                save_unit_id: group.save_unit_id,
                group_id: group.id,
                archive_path: group.archive_path.clone(),
                target_path,
                kind: group.kind,
                delete_before_apply: group.delete_before_apply,
            });
        }
        finish_plan(entries, skipped)
    }
}

fn finish_plan(
    entries: Vec<RestoreEntry>,
    skipped: std::collections::BTreeSet<u32>,
) -> Result<RestorePlan, RestorePlanError> {
    super::restore_targets::validate_targets(&entries)?;
    let skipped_inactive_save_unit_ids = skipped.into_iter().collect::<Vec<_>>();
    if entries.is_empty() && !skipped_inactive_save_unit_ids.is_empty() {
        return Err(RestorePlanError::NoActiveTargetSaveUnits {
            save_unit_ids: skipped_inactive_save_unit_ids,
        });
    }
    Ok(RestorePlan {
        entries,
        skipped_inactive_save_unit_ids,
    })
}

fn selected_candidate<'a>(
    group: &ArchiveCaptureGroup,
    groups: &[ArchiveCaptureGroup],
    report: &'a ResolutionReport,
) -> Result<&'a crate::path_resolution::CandidateExpression, RestorePlanError> {
    // A device-local resource ID is not a cross-device save identity. Every
    // captured file of this Save Unit belongs to the selected game instance.
    if groups.iter().any(|other| {
        other.save_unit_id == group.save_unit_id && other.dimensions != group.dimensions
    }) {
        return Err(RestorePlanError::MultipleSourceInstances {
            save_unit_id: group.save_unit_id,
        });
    }
    if matches!(
        report.selection_state,
        crate::path_resolution::ResolutionSelectionState::Missing
            | crate::path_resolution::ResolutionSelectionState::Ambiguous { .. }
            | crate::path_resolution::ResolutionSelectionState::StaleSelection { .. }
    ) || report.candidates.len() != 1
    {
        return Err(RestorePlanError::SelectionRequired {
            save_unit_id: group.save_unit_id,
        });
    }
    Ok(&report.candidates[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path_resolution::{
        CandidateDimensions, CandidateExpression, ResolutionSelectionState, ResolvedLocationKind,
        ResolvedSaveLocation,
    };

    fn group() -> ArchiveCaptureGroup {
        ArchiveCaptureGroup {
            relative_expression: None,
            id: 2,
            save_unit_id: 7,
            candidate_id: "source".to_string(),
            dimensions: CandidateDimensions {
                root_id: Some("source-root".to_string()),
                ..CandidateDimensions::default()
            },
            relative_path: "Saves/game.dat".to_string(),
            archive_path: "7/2/data/game.dat".to_string(),
            kind: CaptureSourceKind::File,
            delete_before_apply: true,
            source_path_diagnostic: None,
        }
    }

    fn report(candidate_ids: &[(&str, &str)]) -> ResolutionReport {
        ResolutionReport {
            raw_pattern: "<root>/Saves/game.dat".to_string(),
            selection_state: if candidate_ids.len() == 1 {
                ResolutionSelectionState::ImplicitUnique {
                    candidate_id: candidate_ids[0].0.to_string(),
                }
            } else {
                ResolutionSelectionState::Ambiguous {
                    candidate_ids: candidate_ids
                        .iter()
                        .map(|(id, _)| (*id).to_string())
                        .collect(),
                }
            },
            candidates: candidate_ids
                .iter()
                .map(|(id, anchor)| CandidateExpression {
                    variable_pattern: None,
                    id: (*id).to_string(),
                    expression: format!("{anchor}/Saves/game.dat"),
                    logical_anchor: (*anchor).to_string(),
                    dimensions: CandidateDimensions::default(),
                    case_sensitive: false,
                })
                .collect(),
            locations: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn single_instance_restore_rejects_overlapping_save_units() {
        let mut first = group();
        first.kind = CaptureSourceKind::Directory;
        let mut second = group();
        second.save_unit_id = 8;
        let mut folder = report(&[("folder", "D:/Target")]);
        folder.candidates[0].expression = "D:/Target".into();
        let file = report(&[("file", "D:/Target")]);
        let reports = BTreeMap::from([(7, folder), (8, file)]);
        assert!(matches!(
            RestorePlan::build(&[first, second], &reports),
            Err(RestorePlanError::OverlappingTargets { .. })
        ));
    }

    #[test]
    fn single_instance_restore_does_not_merge_archived_accounts() {
        let first = group();
        let mut second = first.clone();
        second.dimensions.account_id = Some("another-account".into());
        let reports = BTreeMap::from([(7, report(&[("one", "D:/Target")]))]);
        assert!(matches!(
            RestorePlan::build(&[first, second], &reports),
            Err(RestorePlanError::MultipleSourceInstances { .. })
        ));
    }

    #[test]
    fn single_instance_restore_does_not_compare_device_local_resource_ids() {
        let mut target = report(&[("a", "C:/A"), ("b", "D:/B")]);
        target.candidates[0].dimensions = group().dimensions;
        let reports = BTreeMap::from([(7, target)]);
        assert!(RestorePlan::build(&[group()], &reports).is_err());
    }

    #[test]
    fn single_instance_restore_keeps_all_wildcard_matches_after_device_change() {
        let mut first = group();
        first.relative_path = "one.sav".into();
        let mut second = first.clone();
        second.id += 1;
        second.relative_path = "two.sav".into();
        second.archive_path = "7/3/data/two.sav".into();
        let mut target = report(&[("new-device", "D:/Target")]);
        target.selection_state = ResolutionSelectionState::ImplicitUnique {
            candidate_id: "new-device".into(),
        };
        target.candidates[0].expression = "D:/Target/*.sav".into();
        let reports = BTreeMap::from([(7, target)]);
        let plan = RestorePlan::build(&[first, second], &reports).unwrap();
        assert_eq!(plan.entries.len(), 2);
        assert_eq!(
            plan.entries[0].target_path,
            PathBuf::from("D:/Target/one.sav")
        );
        assert_eq!(
            plan.entries[1].target_path,
            PathBuf::from("D:/Target/two.sav")
        );
    }

    #[test]
    fn registry_target_uses_current_key_even_when_its_name_changed() {
        let mut capture = group();
        capture.kind = CaptureSourceKind::Registry;
        capture.relative_path = "OldName".into();
        let target = "HKEY_CURRENT_USER/Software/NewName";
        let mut current = report(&[("registry", "HKEY_CURRENT_USER/Software")]);
        current.candidates[0].expression = target.into();
        let reports = BTreeMap::from([(7, current)]);
        for plan in [
            RestorePlan::build(&[capture.clone()], &reports),
            RestorePlan::build_legacy_v2(&[capture], &reports),
        ] {
            assert_eq!(plan.unwrap().entries[0].target_path, PathBuf::from(target));
        }
    }

    #[test]
    fn ambiguous_targets_require_mapping_before_any_restore_entry_exists() {
        let reports = BTreeMap::from([(7, report(&[("a", "C:/A"), ("b", "D:/B")]))]);

        let error = RestorePlan::build(&[group()], &reports).unwrap_err();

        assert_eq!(
            error,
            RestorePlanError::SelectionRequired { save_unit_id: 7 }
        );
    }

    #[test]
    fn a_single_group_uses_an_exact_target_even_when_the_source_name_differs() {
        let mut target = report(&[("target", "D:/Target")]);
        target.candidates[0].expression = "D:/Target/renamed.sav".into();
        let reports = BTreeMap::from([(7, target)]);
        let plan = RestorePlan::build(&[group()], &reports).unwrap();
        assert_eq!(
            plan.entries[0].target_path,
            PathBuf::from("D:/Target/renamed.sav")
        );
    }

    #[test]
    fn archived_groups_without_an_active_save_unit_are_skipped() {
        assert_eq!(
            RestorePlan::build(&[group()], &BTreeMap::new()).unwrap_err(),
            RestorePlanError::NoActiveTargetSaveUnits {
                save_unit_ids: vec![7]
            }
        );
    }

    #[test]
    fn inactive_groups_are_reported_when_active_groups_can_restore() {
        let reports = BTreeMap::from([(7, report(&[("a", "C:/A")]))]);
        let mut inactive = group();
        inactive.save_unit_id = 8;

        let plan = RestorePlan::build(&[group(), inactive], &reports).unwrap();

        assert_eq!(plan.entries.len(), 1);
        assert_eq!(plan.skipped_inactive_save_unit_ids, vec![8]);
    }

    #[test]
    fn legacy_v2_uses_exact_candidate_as_restore_target() {
        let mut legacy = group();
        legacy.archive_path = "7/Saved".to_string();
        legacy.kind = CaptureSourceKind::Directory;
        let reports = BTreeMap::from([(
            7,
            ResolutionReport {
                raw_pattern: "<home>/Saved".to_string(),
                selection_state: ResolutionSelectionState::ImplicitUnique {
                    candidate_id: "home".to_string(),
                },
                candidates: vec![CandidateExpression {
                    variable_pattern: None,
                    id: "home".to_string(),
                    expression: "C:/Users/Player/Saved".to_string(),
                    logical_anchor: "C:/Users/Player".to_string(),
                    dimensions: CandidateDimensions::default(),
                    case_sensitive: false,
                }],
                locations: Vec::new(),
                diagnostics: Vec::new(),
            },
        )]);

        let plan = RestorePlan::build_legacy_v2(&[legacy], &reports).unwrap();

        assert_eq!(
            plan.entries[0].target_path,
            PathBuf::from("C:/Users/Player/Saved")
        );
    }

    #[test]
    fn legacy_v2_rejects_wildcard_target_even_when_it_currently_matches() {
        let reports = BTreeMap::from([(
            7,
            ResolutionReport {
                raw_pattern: "<home>/*.sav".to_string(),
                selection_state: ResolutionSelectionState::ImplicitUnique {
                    candidate_id: "home".to_string(),
                },
                candidates: vec![CandidateExpression {
                    variable_pattern: None,
                    id: "home".to_string(),
                    expression: "C:/Users/Player/*.sav".to_string(),
                    logical_anchor: "C:/Users/Player".to_string(),
                    dimensions: CandidateDimensions::default(),
                    case_sensitive: false,
                }],
                locations: vec![ResolvedSaveLocation {
                    candidate_id: "home".to_string(),
                    path: "C:/Users/Player/slot.sav".to_string(),
                    kind: ResolvedLocationKind::File,
                    logical_anchor: "C:/Users/Player".to_string(),
                    dimensions: CandidateDimensions::default(),
                }],
                diagnostics: Vec::new(),
            },
        )]);

        let error = RestorePlan::build_legacy_v2(&[group()], &reports).unwrap_err();

        assert!(matches!(
            error,
            RestorePlanError::LegacyWildcardTarget {
                save_unit_id: 7,
                ..
            }
        ));
    }
}
