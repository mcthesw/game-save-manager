use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use crate::{
    backup::{CaptureSourceKind, Game},
    path_resolution::model::{first_unescaped_glob, unescape_glob_literal},
};

use super::{ArchiveCaptureGroup, ArchiveManifest, migration::ArchiveMigrationError};

/// Before Save Unit IDs were recorded, a unique historical basename is the
/// only automatic association available. Never choose the first matching unit.
pub(super) fn flat_manifest(
    game: &Game,
    staging: &Path,
    associations: &BTreeMap<String, u32>,
) -> Result<ArchiveManifest, ArchiveMigrationError> {
    let mut groups = Vec::new();
    let mut claimed = BTreeSet::new();
    let mut entries = fs::read_dir(staging)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort();
    for path in entries {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| ArchiveMigrationError::Invalid("non-UTF-8 name".into()))?;
        let candidates: Vec<_> = game
            .save_paths
            .iter()
            .filter(|unit| {
                if let Some(id) = associations.get(name) {
                    return *id == unit.id;
                }
                let matches = |expression: &str| {
                    first_unescaped_glob(expression).is_none()
                        && basename(&unescape_glob_literal(expression)).eq_ignore_ascii_case(name)
                };
                unit.paths()
                    .is_some_and(|paths| paths.values().any(|path| matches(path)))
                    || unit
                        .manifest_pattern()
                        .is_some_and(|(pattern, _)| matches(pattern.raw()))
            })
            .collect();
        let [unit] = candidates.as_slice() else {
            return Err(ArchiveMigrationError::Association(name.into()));
        };
        if !claimed.insert(unit.id) {
            return Err(ArchiveMigrationError::Association(name.into()));
        }
        groups.push(ArchiveCaptureGroup {
            id: groups.len() as u32,
            save_unit_id: unit.id,
            candidate_id: String::new(),
            dimensions: Default::default(),
            relative_expression: None,
            relative_path: String::new(),
            archive_path: name.into(),
            kind: if path.is_dir() {
                CaptureSourceKind::Directory
            } else {
                CaptureSourceKind::File
            },
            delete_before_apply: unit.delete_before_apply,
            source_path_diagnostic: None,
        });
    }
    if groups.is_empty() {
        return Err(ArchiveMigrationError::Invalid("empty archive".into()));
    }
    Ok(ArchiveManifest {
        version: 1,
        groups,
        source_fingerprint: None,
        identity: None,
    })
}

pub(super) fn basename(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
}
