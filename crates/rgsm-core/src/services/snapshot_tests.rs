use std::collections::HashMap;

use super::*;
use crate::backup::SaveUnitSource;
use crate::path_pattern::{ManifestPathConstraints, ManifestPathPattern};
use crate::path_resolution::CandidateDimensions;

fn legacy_v2_group(kind: CaptureSourceKind) -> ArchiveCaptureGroup {
    ArchiveCaptureGroup {
        relative_expression: None,
        id: 0,
        save_unit_id: 12,
        candidate_id: "legacy-v2".to_string(),
        dimensions: CandidateDimensions::default(),
        relative_path: String::new(),
        archive_path: "12/registry.reg".to_string(),
        kind,
        delete_before_apply: false,
        source_path_diagnostic: None,
    }
}

#[test]
fn batch_error_state_retains_the_first_failure() {
    let mut first_error = None;
    retain_first_error(&mut first_error, Err(BackupError::NoDataMatched));
    retain_first_error(&mut first_error, Ok(()));
    retain_first_error(&mut first_error, Err(BackupError::NoBackupAvailable));

    assert!(matches!(first_error, Some(BackupError::NoDataMatched)));
}

#[test]
fn legacy_v2_concrete_type_overrides_archive_filename_inference() {
    let mut groups = vec![legacy_v2_group(CaptureSourceKind::Registry)];
    let units = vec![SaveUnit::concrete(
        12,
        SaveUnitType::File,
        HashMap::new(),
        true,
        true,
    )];

    apply_legacy_v2_save_unit_metadata(&mut groups, &units);

    assert_eq!(groups[0].kind, CaptureSourceKind::File);
    assert!(groups[0].delete_before_apply);
}

#[test]
fn legacy_v2_dynamic_pattern_keeps_archive_shape_inference() {
    let mut groups = vec![legacy_v2_group(CaptureSourceKind::Directory)];
    let units = vec![SaveUnit {
        id: 12,
        source: SaveUnitSource::ManifestPattern {
            expected_type: None,
            pattern: ManifestPathPattern::new("<home>/Saves/*"),
            constraints: ManifestPathConstraints::default(),
        },
        delete_before_apply: true,
        enabled: true,
    }];

    apply_legacy_v2_save_unit_metadata(&mut groups, &units);

    assert_eq!(groups[0].kind, CaptureSourceKind::Directory);
    assert!(groups[0].delete_before_apply);
}

#[test]
fn legacy_v2_typed_pattern_preserves_declared_folder_kind() {
    let mut groups = vec![legacy_v2_group(CaptureSourceKind::File)];
    let units = vec![SaveUnit {
        id: 12,
        source: SaveUnitSource::ManifestPattern {
            expected_type: Some(SaveUnitType::Folder),
            pattern: ManifestPathPattern::new("<root>/Saved"),
            constraints: ManifestPathConstraints::default(),
        },
        delete_before_apply: true,
        enabled: true,
    }];

    apply_legacy_v2_save_unit_metadata(&mut groups, &units);

    assert_eq!(groups[0].kind, CaptureSourceKind::Directory);
    assert!(groups[0].delete_before_apply);
}
