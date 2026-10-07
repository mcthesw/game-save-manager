use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::{
    backup::{
        ArchiveFormat, CaptureGroup, CapturePlan, CaptureSourceKind, CompressionPreset, Game,
        Snapshot, compute_file_hash,
    },
    device::DeviceId,
    preclude::CompressError,
};

use super::{
    ArchiveBackend, ArchiveIdentity, ArchiveManifest, ArchiveVersion, SevenZBackend, ZipBackend,
    legacy_layout::flat_manifest,
    payload::{ArchivePayload, PayloadSource},
    portable::{ArchiveLocation, MANIFEST_ENTRY, prepare_archive},
    staging::{StagingDirectory, extract},
};

#[derive(Debug, Error)]
pub enum ArchiveMigrationError {
    #[error("cannot associate archived location: {0}")]
    Association(String),
    #[error("archive contains multiple instances for save unit {0}")]
    MultipleInstances(u32),
    #[error("archive is invalid: {0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Archive(#[from] CompressError),
}

pub struct ArchiveMigrationInput<'a> {
    pub game: &'a Game,
    pub snapshot: &'a Snapshot,
    pub device_id: &'a DeviceId,
    pub source: &'a Path,
    pub destination: &'a Path,
    pub preset: CompressionPreset,
    pub associations: &'a BTreeMap<String, u32>,
}

/// No live save paths, configuration writes, registry imports or cloud I/O.
pub fn convert_snapshot_archive(
    input: ArchiveMigrationInput<'_>,
) -> Result<u64, ArchiveMigrationError> {
    let original_hash = compute_file_hash(input.source)?;
    if input
        .snapshot
        .archive_hash
        .as_ref()
        .is_some_and(|hash| hash != &original_hash)
    {
        return Err(ArchiveMigrationError::Invalid(
            "source checksum mismatch".into(),
        ));
    }
    let parent = input
        .destination
        .parent()
        .ok_or_else(|| ArchiveMigrationError::Invalid("destination has no parent".into()))?;
    let staging = StagingDirectory::new(parent)?;
    extract(input.source, input.snapshot.archive_format, &staging.0)?;
    let mut legacy = match input.snapshot.archive_format {
        ArchiveFormat::SevenZ => SevenZBackend.read_capture_manifest(input.source)?,
        ArchiveFormat::Zip => match ZipBackend.archive_version(input.source)? {
            ArchiveVersion::Legacy | ArchiveVersion::V1 => {
                flat_manifest(input.game, &staging.0, input.associations)?
            }
            _ => ZipBackend.read_capture_manifest(input.source)?,
        },
    };
    if legacy.version == 2 {
        for group in &mut legacy.groups {
            if let Some(unit) = input
                .game
                .save_paths
                .iter()
                .find(|unit| unit.id == group.save_unit_id)
            {
                if unit.unit_type() == Some(&crate::backup::SaveUnitType::WinRegistry) {
                    group.kind = CaptureSourceKind::Registry;
                } else if group.kind == CaptureSourceKind::Registry {
                    group.kind = CaptureSourceKind::File;
                }
            }
        }
    }
    validate_group_coverage(&legacy, &staging.0)?;
    let mut identity = legacy.identity.clone().unwrap_or_else(|| {
        let mut identity = ArchiveIdentity::for_snapshot(
            input.game,
            input.snapshot,
            input.snapshot.device_id.as_ref().unwrap_or(input.device_id),
        );
        identity.recovered_metadata = true;
        identity
    });
    if identity.game_id != input.game.backup_dir_name().as_ref()
        || identity.snapshot_id != input.snapshot.date
    {
        return Err(ArchiveMigrationError::Invalid(
            "embedded identity does not match catalog".into(),
        ));
    }
    let mut groups = Vec::new();
    let mut registry_bytes = BTreeMap::new();
    for group in &legacy.groups {
        let source = staging.0.join(&group.archive_path);
        let unit = input
            .game
            .save_paths
            .iter()
            .find(|unit| unit.id == group.save_unit_id);
        if legacy.version <= 2 {
            let unit =
                unit.ok_or_else(|| ArchiveMigrationError::Association(group.archive_path.clone()))?;
            let expression = identity
                .locations
                .iter()
                .find(|location| location.save_unit_id == unit.id)
                .map(|location| location.expression.as_str())
                .unwrap_or("");
            if crate::path_resolution::model::first_unescaped_glob(expression).is_some() {
                return Err(ArchiveMigrationError::Association(
                    group.archive_path.clone(),
                ));
            }
        }
        if !identity
            .locations
            .iter()
            .any(|location| location.save_unit_id == group.save_unit_id)
        {
            identity.locations.push(ArchiveLocation {
                save_unit_id: group.save_unit_id,
                expression: group.source_path_diagnostic.clone().unwrap_or_default(),
            });
        }
        let mut anchor = original_anchor(group, &source);
        if group.kind == CaptureSourceKind::Registry {
            let bytes = fs::read(&source)?;
            let data = if group.archive_path.ends_with(".json") {
                serde_json::from_slice(&bytes)
                    .map_err(|error| ArchiveMigrationError::Invalid(error.to_string()))?
            } else {
                crate::backup::registry::deserialize_reg_file(&bytes)
                    .map_err(|error| ArchiveMigrationError::Invalid(error.to_string()))?
            };
            anchor = PathBuf::from(&data.root_key);
            registry_bytes.insert(
                (group.save_unit_id, group.id),
                crate::backup::registry::serialize_reg_file(&data)
                    .map_err(|error| ArchiveMigrationError::Invalid(error.to_string()))?,
            );
        }
        groups.push(CaptureGroup {
            id: group.id,
            save_unit_id: group.save_unit_id,
            candidate_id: String::new(),
            dimensions: Default::default(),
            relative_expression: group.relative_expression.clone(),
            logical_anchor: anchor,
            source_path: source.to_string_lossy().into_owned(),
            relative_path: group.relative_path.clone(),
            archive_path: group.archive_path.clone(),
            kind: group.kind,
            delete_before_apply: if legacy.version <= 2 {
                unit.is_some_and(|unit| unit.delete_before_apply)
            } else {
                group.delete_before_apply
            },
        });
    }
    let (plan, mut manifest) = prepare_archive(
        &CapturePlan { groups },
        identity,
        legacy.source_fingerprint.clone(),
    )?;
    if plan.groups.len() != legacy.groups.len() {
        return Err(ArchiveMigrationError::Association(
            "overlapping historical locations".into(),
        ));
    }
    for group in &mut manifest.groups {
        group.source_path_diagnostic = legacy
            .groups
            .iter()
            .find(|old| old.save_unit_id == group.save_unit_id && old.id == group.id)
            .and_then(|old| old.source_path_diagnostic.clone());
    }
    let payloads: Vec<_> = plan
        .groups
        .iter()
        .map(|group| ArchivePayload {
            name: group.archive_path.clone(),
            source: registry_bytes
                .remove(&(group.save_unit_id, group.id))
                .map(PayloadSource::Bytes)
                .unwrap_or_else(|| PayloadSource::FileSystem(group.source_path.clone().into())),
        })
        .collect();
    let existed = input.destination.exists();
    let result = (|| {
        let size = if existed {
            fs::metadata(input.destination)?.len()
        } else {
            super::seven_z::write_atomic(
                &payloads,
                input.destination,
                input.preset,
                &manifest,
                MANIFEST_ENTRY,
            )?
        };
        super::migration_verify::verify_payloads(&payloads, input.destination, &manifest)?;
        if compute_file_hash(input.source)? != original_hash {
            return Err(ArchiveMigrationError::Invalid(
                "source changed during conversion".into(),
            ));
        }
        Ok(size)
    })();
    if result.is_err() && !existed {
        let _ = fs::remove_file(input.destination);
    }
    result
}

pub fn inspect_archive_size(
    path: &Path,
    format: ArchiveFormat,
) -> Result<u64, ArchiveMigrationError> {
    match format {
        ArchiveFormat::Zip => {
            let mut zip = zip::ZipArchive::new(fs::File::open(path)?)
                .map_err(|error| ArchiveMigrationError::Invalid(error.to_string()))?;
            let mut size = 0_u64;
            for index in 0..zip.len() {
                size = size.saturating_add(
                    zip.by_index(index)
                        .map_err(|error| ArchiveMigrationError::Invalid(error.to_string()))?
                        .size(),
                );
            }
            Ok(size)
        }
        ArchiveFormat::SevenZ => {
            let reader = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())
                .map_err(|error| ArchiveMigrationError::Invalid(error.to_string()))?;
            Ok(reader
                .archive()
                .files
                .iter()
                .fold(0_u64, |size, entry| size.saturating_add(entry.size)))
        }
    }
}

fn original_anchor(group: &super::ArchiveCaptureGroup, source: &Path) -> PathBuf {
    let origin = group
        .source_path_diagnostic
        .as_deref()
        .unwrap_or(&group.archive_path)
        .replace('\\', "/");
    if group.relative_path.is_empty() {
        return origin.into();
    }
    let suffix = format!("/{}", group.relative_path.replace('\\', "/"));
    origin
        .strip_suffix(&suffix)
        .map(PathBuf::from)
        .unwrap_or_else(|| source.parent().unwrap_or(source).to_path_buf())
}

fn validate_group_coverage(
    manifest: &ArchiveManifest,
    root: &Path,
) -> Result<(), ArchiveMigrationError> {
    for group in &manifest.groups {
        super::portable::validate_relative_name(&group.archive_path)?;
        for other in &manifest.groups {
            if group.save_unit_id == other.save_unit_id && group.dimensions != other.dimensions {
                return Err(ArchiveMigrationError::MultipleInstances(group.save_unit_id));
            }
        }
        let path = root.join(&group.archive_path);
        if (group.kind == CaptureSourceKind::Directory && !path.is_dir())
            || (group.kind != CaptureSourceKind::Directory && !path.is_file())
        {
            return Err(ArchiveMigrationError::Invalid(
                "capture entry is missing".into(),
            ));
        }
    }
    for entry in walkdir::WalkDir::new(root).min_depth(1) {
        let entry = entry.map_err(|error| ArchiveMigrationError::Invalid(error.to_string()))?;
        let relative = entry
            .path()
            .strip_prefix(root)
            .expect("staging child")
            .to_string_lossy()
            .replace('\\', "/");
        if relative == "_rgsm"
            || relative.starts_with("_rgsm/")
            || (manifest.version >= 6 && relative == "RESTORE.txt")
        {
            continue;
        }
        let covered = manifest.groups.iter().any(|group| {
            relative == group.archive_path
                || (group.kind == CaptureSourceKind::Directory
                    && relative.starts_with(&format!("{}/", group.archive_path)))
                || (entry.file_type().is_dir()
                    && group.archive_path.starts_with(&format!("{relative}/")))
        });
        if !covered {
            return Err(ArchiveMigrationError::Association(relative));
        }
    }
    Ok(())
}
