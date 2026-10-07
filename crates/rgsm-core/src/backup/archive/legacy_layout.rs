use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use crate::{
    backup::{CaptureSourceKind, Game},
    path_resolution::model::{first_unescaped_glob, unescape_glob_literal},
};

use super::{ArchiveCaptureGroup, ArchiveManifestV3};

pub(super) fn read_flat_zip_manifest(
    game: &Game,
    archive: &Path,
) -> Result<ArchiveManifestV3, crate::preclude::CompressError> {
    use crate::preclude::{BackupFileError, CompressError};
    let read = || -> Result<_, anyhow::Error> {
        let mut zip = zip::ZipArchive::new(fs::File::open(archive)?)
            .map_err(|error| anyhow::anyhow!(error))?;
        let mut entries = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for index in 0..zip.len() {
            let entry = zip
                .by_index(index)
                .map_err(|error| anyhow::anyhow!(error))?;
            let name = entry.name().trim_end_matches('/');
            if entry.enclosed_name().is_none()
                || name.is_empty()
                || name.replace('\\', "/").split('/').any(|part| {
                    part.is_empty() || part == "." || part == ".." || part.contains(':')
                })
            {
                return Err(anyhow::anyhow!("unsafe archive path: {name}"));
            }
            if !seen.insert(name.to_string()) {
                return Err(anyhow::anyhow!("duplicate entry"));
            }
            let mut parts = name.split('/');
            let root = parts.next().unwrap_or_default().to_string();
            let directory = entry.is_dir() || parts.next().is_some();
            if entries
                .insert(root, directory)
                .is_some_and(|previous| previous != directory)
            {
                return Err(anyhow::anyhow!("conflicting archive entries"));
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
) -> Result<ArchiveManifestV3, anyhow::Error> {
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
            return Err(anyhow::anyhow!("cannot associate archive entry {name}"));
        };
        if !claimed.insert(unit.id) {
            return Err(anyhow::anyhow!("cannot associate archive entry {name}"));
        }
        groups.push(ArchiveCaptureGroup {
            id: groups.len() as u32,
            save_unit_id: unit.id,
            candidate_id: String::new(),
            dimensions: Default::default(),
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
        return Err(anyhow::anyhow!("empty archive"));
    }
    Ok(ArchiveManifestV3 { version: 1, groups })
}

pub(super) fn basename(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
}
