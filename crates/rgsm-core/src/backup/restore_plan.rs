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
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RestorePlanError {
    #[error("restore mapping is required for save unit {save_unit_id}, capture group {group_id}")]
    MappingRequired { save_unit_id: u32, group_id: u32 },
    #[error("restore mapping for save unit {save_unit_id}, capture group {group_id} is stale")]
    StaleMapping { save_unit_id: u32, group_id: u32 },
}

impl RestorePlan {
    pub fn build(
        groups: &[ArchiveCaptureGroup],
        reports: &BTreeMap<u32, ResolutionReport>,
        rules: &[RestoreMappingRule],
    ) -> Result<Self, RestorePlanError> {
        let mut entries = Vec::new();
        for group in groups {
            let Some(report) = reports.get(&group.save_unit_id) else {
                continue;
            };
            let candidates = report.candidates.as_slice();
            let rule = rules.iter().find(|rule| {
                rule.save_unit_id == group.save_unit_id
                    && rule.source_dimensions == group.dimensions
            });
            let selected = if let Some(rule) = rule {
                let selected = candidates
                    .iter()
                    .filter(|candidate| rule.target_candidate_ids.contains(&candidate.id))
                    .collect::<Vec<_>>();
                if selected.len() != rule.target_candidate_ids.len() {
                    return Err(RestorePlanError::StaleMapping {
                        save_unit_id: group.save_unit_id,
                        group_id: group.id,
                    });
                }
                selected
            } else if let Some(equivalent) = candidates
                .iter()
                .find(|candidate| candidate.dimensions == group.dimensions)
            {
                vec![equivalent]
            } else if candidates.len() == 1 {
                vec![&candidates[0]]
            } else {
                return Err(RestorePlanError::MappingRequired {
                    save_unit_id: group.save_unit_id,
                    group_id: group.id,
                });
            };
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
        Ok(Self { entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path_resolution::{
        CandidateDimensions, CandidateExpression, ResolutionSelectionState,
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
                group_id: 2
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
    fn archived_groups_without_an_active_save_unit_are_skipped() {
        let plan = RestorePlan::build(&[group()], &BTreeMap::new(), &[]).unwrap();
        assert!(plan.entries.is_empty());
    }
}
