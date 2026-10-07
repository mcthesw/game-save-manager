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
    let entries = fs::read_dir(staging)?
        .map(|entry| {
            let entry = entry?;
            Ok((
                entry.file_name().to_string_lossy().into_owned(),
                entry.file_type()?.is_dir(),
            ))
        })
        .collect::<std::io::Result<BTreeMap<_, _>>>()?;
    flat_manifest_from_entries(game, entries, associations)
}

pub(super) fn read_flat_zip_manifest(
    game: &Game,
    archive: &Path,
) -> Result<ArchiveManifest, crate::preclude::CompressError> {
    use crate::preclude::{BackupFileError, CompressError};
    let read = || -> Result<_, ArchiveMigrationError> {
        let mut zip = zip::ZipArchive::new(fs::File::open(archive)?)
            .map_err(|error| ArchiveMigrationError::Invalid(error.to_string()))?;
        let mut entries = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for index in 0..zip.len() {
            let entry = zip
                .by_index(index)
                .map_err(|error| ArchiveMigrationError::Invalid(error.to_string()))?;
            let name = entry.name().trim_end_matches('/');
            super::portable::validate_relative_name(name)?;
            if !seen.insert(name.to_string()) {
                return Err(ArchiveMigrationError::Invalid("duplicate entry".into()));
            }
            let mut parts = name.split('/');
            let root = parts.next().unwrap_or_default().to_string();
            let directory = entry.is_dir() || parts.next().is_some();
            if entries
                .insert(root, directory)
                .is_some_and(|previous| previous != directory)
            {
                return Err(ArchiveMigrationError::Invalid(
                    "conflicting archive entries".into(),
                ));
            }
        }
        flat_manifest_from_entries(game, entries, &BTreeMap::new())
    };
    read().map_err(|error| {
        CompressError::Single(BackupFileError::Unexpected(anyhow::anyhow!(
            rust_i18n::t!(
                "portable_archive.legacy_association_required",
                reason = error.to_string()
            )
            .into_owned()
        )))
    })
}

fn flat_manifest_from_entries(
    game: &Game,
    entries: BTreeMap<String, bool>,
    associations: &BTreeMap<String, u32>,
) -> Result<ArchiveManifest, ArchiveMigrationError> {
    let mut groups = Vec::new();
    let mut claimed = BTreeSet::new();
    for (name, directory) in entries {
        let candidates: Vec<_> = game
            .save_paths
            .iter()
            .filter(|unit| {
                if let Some(id) = associations.get(&name) {
                    return *id == unit.id;
                }
                let matches = |expression: &str| {
                    first_unescaped_glob(expression).is_none()
                        && basename(&unescape_glob_literal(expression)).eq_ignore_ascii_case(&name)
                };
                unit.paths()
                    .is_some_and(|paths| paths.values().any(|path| matches(path)))
                    || unit
                        .manifest_pattern()
                        .is_some_and(|(pattern, _)| matches(pattern.raw()))
            })
            .collect();
        let [unit] = candidates.as_slice() else {
            return Err(ArchiveMigrationError::Association(name));
        };
        if !claimed.insert(unit.id) {
            return Err(ArchiveMigrationError::Association(name));
        }
        groups.push(ArchiveCaptureGroup {
            id: groups.len() as u32,
            save_unit_id: unit.id,
            candidate_id: String::new(),
            dimensions: Default::default(),
            relative_expression: None,
            relative_path: String::new(),
            archive_path: name,
            kind: if directory {
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
