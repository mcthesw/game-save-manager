use std::collections::BTreeMap;
use std::path::PathBuf;

use thiserror::Error;

use crate::path_resolution::ResolutionReport;

use super::archive::ArchiveCaptureGroup;
use super::{CaptureSourceKind, RestoreMappingRule};

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
    #[error("restore mapping is required for save unit {save_unit_id}, capture group {group_id}")]
    MappingRequired {
        save_unit_id: u32,
        group_id: u32,
        source_dimensions: crate::path_resolution::CandidateDimensions,
    },
    #[error("restore mapping for save unit {save_unit_id}, capture group {group_id} is stale")]
    StaleMapping {
        save_unit_id: u32,
        group_id: u32,
        source_dimensions: crate::path_resolution::CandidateDimensions,
    },
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
        rules: &[RestoreMappingRule],
    ) -> Result<Self, RestorePlanError> {
        let mut entries = Vec::new();
        let mut skipped = std::collections::BTreeSet::new();
        for group in groups {
            let Some(report) = reports.get(&group.save_unit_id) else {
                skipped.insert(group.save_unit_id);
                continue;
            };
            let selected = selected_candidates(group, groups, report, rules)?;
            for candidate in selected {
                entries.push(RestoreEntry {
                    save_unit_id: group.save_unit_id,
                    group_id: group.id,
                    archive_path: group.archive_path.clone(),
                    target_path: PathBuf::from(&candidate.logical_anchor)
                        .join(PathBuf::from(&group.relative_path)),
                    kind: group.kind,
                    delete_before_apply: group.delete_before_apply,
                });
            }
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
        rules: &[RestoreMappingRule],
    ) -> Result<Self, RestorePlanError> {
        let mut entries = Vec::new();
        let mut skipped = std::collections::BTreeSet::new();
        for group in groups {
            let Some(report) = reports.get(&group.save_unit_id) else {
                skipped.insert(group.save_unit_id);
                continue;
            };
            let selected = selected_candidates(group, groups, report, rules)?;
            for candidate in selected {
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
        }
        finish_plan(entries, skipped)
    }
}

fn finish_plan(
    entries: Vec<RestoreEntry>,
    skipped: std::collections::BTreeSet<u32>,
) -> Result<RestorePlan, RestorePlanError> {
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

fn selected_candidates<'a>(
    group: &ArchiveCaptureGroup,
    groups: &[ArchiveCaptureGroup],
    report: &'a ResolutionReport,
    rules: &[RestoreMappingRule],
) -> Result<Vec<&'a crate::path_resolution::CandidateExpression>, RestorePlanError> {
    let source_group_count = groups
        .iter()
        .filter(|candidate| candidate.save_unit_id == group.save_unit_id)
        .count();
    let candidates = report.candidates.as_slice();
    let rule = rules.iter().find(|rule| {
        rule.save_unit_id == group.save_unit_id && rule.source_dimensions == group.dimensions
    });
    if let Some(rule) = rule {
        let selected = candidates
            .iter()
            .filter(|candidate| rule.target_candidate_ids.contains(&candidate.id))
            .collect::<Vec<_>>();
        if selected.len() != rule.target_candidate_ids.len() {
            return Err(RestorePlanError::StaleMapping {
                save_unit_id: group.save_unit_id,
                group_id: group.id,
                source_dimensions: group.dimensions.clone(),
            });
        }
        return Ok(selected);
    }
    if let Some(equivalent) = candidates
        .iter()
        .find(|candidate| candidate.dimensions == group.dimensions)
    {
        return Ok(vec![equivalent]);
    }
    if candidates.len() == 1 && source_group_count == 1 {
        return Ok(vec![&candidates[0]]);
    }
    Err(RestorePlanError::MappingRequired {
        save_unit_id: group.save_unit_id,
        group_id: group.id,
        source_dimensions: group.dimensions.clone(),
    })
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
            selection_state: ResolutionSelectionState::Ambiguous {
                candidate_ids: candidate_ids
                    .iter()
                    .map(|(id, _)| (*id).to_string())
                    .collect(),
            },
            candidates: candidate_ids
                .iter()
                .map(|(id, anchor)| CandidateExpression {
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
    fn ambiguous_targets_require_mapping_before_any_restore_entry_exists() {
        let reports = BTreeMap::from([(7, report(&[("a", "C:/A"), ("b", "D:/B")]))]);

        let error = RestorePlan::build(&[group()], &reports, &[]).unwrap_err();

        assert_eq!(
            error,
            RestorePlanError::MappingRequired {
                save_unit_id: 7,
                group_id: 2,
                source_dimensions: group().dimensions,
            }
        );
    }

    #[test]
    fn explicit_rule_can_restore_one_source_group_to_selected_target() {
        let reports = BTreeMap::from([(7, report(&[("a", "C:/A"), ("b", "D:/B")]))]);
        let rules = vec![RestoreMappingRule {
            save_unit_id: 7,
            source_dimensions: group().dimensions,
            target_candidate_ids: vec!["b".to_string()],
        }];

        let plan = RestorePlan::build(&[group()], &reports, &rules).unwrap();

        assert_eq!(plan.entries.len(), 1);
        assert_eq!(
            plan.entries[0].target_path,
            PathBuf::from("D:/B/Saves/game.dat")
        );
        assert!(plan.entries[0].delete_before_apply);
    }

    #[test]
    fn saved_mapping_ignores_unrelated_new_candidates() {
        let reports = BTreeMap::from([(
            7,
            report(&[("a", "C:/A"), ("b", "D:/B"), ("new", "E:/New")]),
        )]);
        let rules = vec![RestoreMappingRule {
            save_unit_id: 7,
            source_dimensions: group().dimensions,
            target_candidate_ids: vec!["b".to_string()],
        }];

        let plan = RestorePlan::build(&[group()], &reports, &rules).unwrap();

        assert_eq!(plan.entries.len(), 1);
        assert_eq!(
            plan.entries[0].target_path,
            PathBuf::from("D:/B/Saves/game.dat")
        );
    }

    #[test]
    fn disappeared_saved_target_is_stale_before_restore() {
        let reports = BTreeMap::from([(7, report(&[("a", "C:/A")]))]);
        let rules = vec![RestoreMappingRule {
            save_unit_id: 7,
            source_dimensions: group().dimensions.clone(),
            target_candidate_ids: vec!["missing".to_string()],
        }];

        let error = RestorePlan::build(&[group()], &reports, &rules).unwrap_err();

        assert!(matches!(
            error,
            RestorePlanError::StaleMapping {
                save_unit_id: 7,
                ..
            }
        ));
    }

    #[test]
    fn archived_groups_without_an_active_save_unit_are_skipped() {
        assert_eq!(
            RestorePlan::build(&[group()], &BTreeMap::new(), &[]).unwrap_err(),
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

        let plan = RestorePlan::build(&[group(), inactive], &reports, &[]).unwrap();

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

        let plan = RestorePlan::build_legacy_v2(&[legacy], &reports, &[]).unwrap();

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

        let error = RestorePlan::build_legacy_v2(&[group()], &reports, &[]).unwrap_err();

        assert!(matches!(
            error,
            RestorePlanError::LegacyWildcardTarget {
                save_unit_id: 7,
                ..
            }
        ));
    }
}
