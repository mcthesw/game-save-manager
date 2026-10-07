use std::{fs::File, io::Read, path::Path};

use sevenz_rust2::{ArchiveReader, Password};

use crate::preclude::CompressError;

use super::{
    ArchiveManifest, V4_MANIFEST_ENTRY,
    manifest::V5_MANIFEST_ENTRY,
    portable::{CURRENT_VERSION, MANIFEST_ENTRY, validate_relative_name},
};

pub(super) fn read_manifest(path: &Path) -> Result<ArchiveManifest, CompressError> {
    let file = File::open(path).map_err(|error| CompressError::Single(error.into()))?;
    let mut reader = ArchiveReader::new(file, Password::empty()).map_err(error)?;
    let mut bytes = None;
    let mut name = String::new();
    reader
        .for_each_entries(|entry, source| {
            if matches!(
                entry.name(),
                V4_MANIFEST_ENTRY | V5_MANIFEST_ENTRY | MANIFEST_ENTRY
            ) {
                if bytes.is_some() {
                    return Err(std::io::Error::other("duplicate archive manifest").into());
                }
                let mut value = Vec::new();
                source.take(16 * 1024 * 1024 + 1).read_to_end(&mut value)?;
                if value.len() > 16 * 1024 * 1024 {
                    return Err(std::io::Error::other("archive manifest is too large").into());
                }
                bytes = Some(value);
                name = entry.name().to_string();
            }
            Ok(true)
        })
        .map_err(error)?;
    let bytes = bytes.ok_or_else(|| invalid("archive manifest is missing"))?;
    let manifest: ArchiveManifest =
        serde_json::from_slice(&bytes).map_err(|error| invalid(&error.to_string()))?;
    let expected_version = match name.as_str() {
        V4_MANIFEST_ENTRY => 4,
        V5_MANIFEST_ENTRY => 5,
        _ => CURRENT_VERSION,
    };
    if manifest.version != expected_version {
        return Err(invalid("unsupported archive manifest version"));
    }
    if manifest.version == CURRENT_VERSION {
        let identity = manifest
            .identity
            .as_ref()
            .ok_or_else(|| invalid("archive identity is missing"))?;
        if identity.game_id.is_empty()
            || identity.snapshot_id.is_empty()
            || manifest.groups.is_empty()
        {
            return Err(invalid("archive identity or save data is empty"));
        }
        let mut names = std::collections::BTreeSet::new();
        for group in &manifest.groups {
            validate_relative_name(&group.archive_path)?;
            if !group.relative_path.is_empty() {
                validate_relative_name(&group.relative_path)?;
            }
            if !names.insert(group.archive_path.to_lowercase())
                || !identity
                    .locations
                    .iter()
                    .any(|location| location.save_unit_id == group.save_unit_id)
            {
                return Err(invalid("archive locations are inconsistent"));
            }
        }
    }
    Ok(manifest)
}

fn error(error: sevenz_rust2::Error) -> CompressError {
    CompressError::Unexpected(error.into())
}

fn invalid(message: &str) -> CompressError {
    CompressError::Unexpected(anyhow::anyhow!(message.to_string()))
}
