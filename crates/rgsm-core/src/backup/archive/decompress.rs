//! ZIP archive extraction and save unit restoration.
//!
//! All ZIP formats use the same explicit RestorePlan. Historical names are
//! normalized into the manifest before resolving the current device's targets.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use filetime::{FileTime, set_file_mtime};

use crate::{
    backup::{CaptureSourceKind, RestorePlan},
    preclude::*,
};

/// Notification level for restore progress messages.
#[derive(Debug, Clone, Copy)]
pub enum RestoreNotificationLevel {
    Info,
    Warning,
}

/// Trait for receiving notifications during archive restoration.
///
/// GUI implements this to emit IPC events; CLI might log to stdout.
/// Passed as `Option<&dyn RestoreNotifier>` so callers without UI can pass `None`.
pub trait RestoreNotifier: Send + Sync {
    fn notify(&self, level: RestoreNotificationLevel, title: &str, msg: &str);
}

use super::{
    ArchiveCaptureGroup, ArchiveManifestV3, V3_MANIFEST_ENTRY,
    timestamp::zip_datetime_to_system_time, version::ArchiveVersion,
};

pub(super) fn archive_version(archive_path: &Path) -> Result<ArchiveVersion, CompressError> {
    let file = File::open(archive_path).map_err(|error| CompressError::Single(error.into()))?;
    let zip = zip::ZipArchive::new(file).map_err(|error| CompressError::Single(error.into()))?;
    Ok(ArchiveVersion::from_comment(zip.comment()))
}

pub(super) fn read_capture_manifest(
    archive_path: &Path,
) -> Result<ArchiveManifestV3, CompressError> {
    let file = File::open(archive_path).map_err(|error| CompressError::Single(error.into()))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|error| CompressError::Single(error.into()))?;
    match ArchiveVersion::from_comment(zip.comment()) {
        ArchiveVersion::V2 => read_v2_capture_manifest(&mut zip),
        ArchiveVersion::V3 => serde_json::from_reader(
            zip.by_name(V3_MANIFEST_ENTRY)
                .map_err(|error| CompressError::Single(error.into()))?,
        )
        .map_err(|error| CompressError::Single(BackupFileError::Unexpected(error.into()))),
        _ => Err(CompressError::Single(BackupFileError::Unexpected(
            anyhow::anyhow!("capture manifest requested from an archive without save-unit IDs"),
        ))),
    }
}

fn read_v2_capture_manifest(
    zip: &mut zip::ZipArchive<File>,
) -> Result<ArchiveManifestV3, CompressError> {
    #[derive(Default)]
    struct LegacyGroup {
        root: Option<String>,
        directory: bool,
    }

    let mut units = BTreeMap::<u32, LegacyGroup>::new();
    for index in 0..zip.len() {
        let entry = zip
            .by_index(index)
            .map_err(|error| CompressError::Single(error.into()))?;
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        let mut components = path.components();
        let Some(std::path::Component::Normal(unit)) = components.next() else {
            continue;
        };
        let Some(std::path::Component::Normal(root)) = components.next() else {
            continue;
        };
        let Some(unit) = unit.to_str().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        let root = root.to_string_lossy().into_owned();
        let directory = entry.is_dir() || components.next().is_some();
        let group = units.entry(unit).or_default();
        if let Some(existing) = group.root.as_deref().filter(|existing| *existing != root) {
            // V2 readers preferred .reg when a legacy JSON fallback was also
            // present. Preserve that format rule at this reader boundary.
            let registry_pair = [existing, root.as_str()];
            if registry_pair.contains(&crate::backup::registry::REGISTRY_DATA_FILENAME)
                && registry_pair.contains(&crate::backup::registry::LEGACY_REGISTRY_DATA_FILENAME)
                && !group.directory
                && !directory
            {
                group.root = Some(crate::backup::registry::REGISTRY_DATA_FILENAME.into());
                continue;
            }
            return Err(CompressError::Single(BackupFileError::Unexpected(
                anyhow::anyhow!("Archive V2 save unit {unit} contains multiple roots"),
            )));
        }
        group.root = Some(root);
        group.directory |= directory;
    }

    let groups = units
        .into_iter()
        .filter_map(|(save_unit_id, group)| {
            let root = group.root?;
            let kind = if root == crate::backup::registry::REGISTRY_DATA_FILENAME
                || root == crate::backup::registry::LEGACY_REGISTRY_DATA_FILENAME
            {
                CaptureSourceKind::Registry
            } else if group.directory {
                CaptureSourceKind::Directory
            } else {
                CaptureSourceKind::File
            };
            Some(ArchiveCaptureGroup {
                id: 0,
                save_unit_id,
                candidate_id: "legacy-v2".to_string(),
                dimensions: Default::default(),
                relative_path: String::new(),
                archive_path: format!("{save_unit_id}/{root}"),
                kind,
                delete_before_apply: false,
                source_path_diagnostic: None,
            })
        })
        .collect::<Vec<_>>();
    if groups.is_empty() {
        return Err(CompressError::Single(BackupFileError::Unexpected(
            anyhow::anyhow!("Archive V2 contains no save-unit entries"),
        )));
    }
    Ok(ArchiveManifestV3 { version: 2, groups })
}

pub(super) fn restore_capture_plan(
    plan: &RestorePlan,
    archive_path: &Path,
) -> Result<(), CompressError> {
    let file = File::open(archive_path).map_err(|error| CompressError::Single(error.into()))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|error| CompressError::Single(error.into()))?;
    let version = ArchiveVersion::from_comment(zip.comment());
    verify_capture_entries(&mut zip, plan)?;
    // Validate the compressed payload before replacing any live files. Merely
    // finding names in the ZIP directory does not establish that its data reads.
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| CompressError::Single(error.into()))?;
        if entry.enclosed_name().is_none() {
            return Err(CompressError::Unexpected(anyhow::anyhow!(
                "unsafe archive path"
            )));
        }
        if plan.entries.iter().any(|planned| {
            planned.kind == CaptureSourceKind::Registry && planned.archive_path == entry.name()
        }) {
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|error| CompressError::Single(error.into()))?;
            super::payload::read_registry_payload(entry.name(), &bytes)?;
        } else {
            std::io::copy(&mut entry, &mut std::io::sink())
                .map_err(|error| CompressError::Single(error.into()))?;
        }
    }
    for entry in &plan.entries {
        if entry.delete_before_apply && entry.target_path.exists() {
            let result = if entry.target_path.is_dir() {
                fs::remove_dir_all(&entry.target_path)
            } else {
                fs::remove_file(&entry.target_path)
            };
            result.map_err(|error| CompressError::Single(error.into()))?;
        }
        match entry.kind {
            CaptureSourceKind::Registry => restore_registry_capture(&mut zip, entry)?,
            CaptureSourceKind::File => restore_file_capture(&mut zip, entry, version)?,
            CaptureSourceKind::Directory => restore_directory_capture(&mut zip, entry, version)?,
        }
    }
    Ok(())
}

fn verify_capture_entries(
    zip: &mut zip::ZipArchive<File>,
    plan: &RestorePlan,
) -> Result<(), CompressError> {
    for entry in &plan.entries {
        match entry.kind {
            CaptureSourceKind::File | CaptureSourceKind::Registry => {
                zip.by_name(&entry.archive_path)
                    .map_err(|error| CompressError::Single(error.into()))?;
            }
            CaptureSourceKind::Directory => {
                let prefix = format!("{}/", entry.archive_path.trim_end_matches('/'));
                if !zip.file_names().any(|name| name.starts_with(&prefix)) {
                    return Err(CompressError::Single(BackupFileError::Unexpected(
                        anyhow::anyhow!("capture directory is missing from archive: {prefix}"),
                    )));
                }
            }
        }
    }
    Ok(())
}

fn restore_file_capture(
    zip: &mut zip::ZipArchive<File>,
    entry: &crate::backup::RestoreEntry,
    version: ArchiveVersion,
) -> Result<(), CompressError> {
    let mut source = zip
        .by_name(&entry.archive_path)
        .map_err(|error| CompressError::Single(error.into()))?;
    if let Some(parent) = entry.target_path.parent() {
        fs::create_dir_all(parent).map_err(|error| CompressError::Single(error.into()))?;
    }
    let mut target =
        File::create(&entry.target_path).map_err(|error| CompressError::Single(error.into()))?;
    std::io::copy(&mut source, &mut target).map_err(|error| CompressError::Single(error.into()))?;
    if let Some(zip_time) = source.last_modified() {
        let _ = set_file_mtime(
            &entry.target_path,
            FileTime::from_system_time(zip_datetime_to_system_time(zip_time, version)),
        );
    }
    Ok(())
}

fn restore_directory_capture(
    zip: &mut zip::ZipArchive<File>,
    entry: &crate::backup::RestoreEntry,
    version: ArchiveVersion,
) -> Result<(), CompressError> {
    let prefix = format!("{}/", entry.archive_path.trim_end_matches('/'));
    let mut directory_times = Vec::new();
    for index in 0..zip.len() {
        let mut source = zip
            .by_index(index)
            .map_err(|error| CompressError::Single(error.into()))?;
        let Some(relative) = source.name().strip_prefix(&prefix) else {
            continue;
        };
        let relative = PathBuf::from(relative);
        if relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        }) {
            continue;
        }
        let target = entry.target_path.join(&relative);
        if source.is_dir() {
            fs::create_dir_all(&target).map_err(|error| CompressError::Single(error.into()))?;
            if let Some(zip_time) = source.last_modified() {
                directory_times.push((
                    target,
                    FileTime::from_system_time(zip_datetime_to_system_time(zip_time, version)),
                ));
            }
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| CompressError::Single(error.into()))?;
        }
        let mut output =
            File::create(target).map_err(|error| CompressError::Single(error.into()))?;
        std::io::copy(&mut source, &mut output)
            .map_err(|error| CompressError::Single(error.into()))?;
        if let Some(zip_time) = source.last_modified() {
            let _ = set_file_mtime(
                entry.target_path.join(relative),
                FileTime::from_system_time(zip_datetime_to_system_time(zip_time, version)),
            );
        }
    }
    directory_times.sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
    for (path, time) in directory_times {
        set_file_mtime(path, time).map_err(|error| CompressError::Single(error.into()))?;
    }
    Ok(())
}

fn restore_registry_capture(
    zip: &mut zip::ZipArchive<File>,
    entry: &crate::backup::RestoreEntry,
) -> Result<(), CompressError> {
    let mut source = zip
        .by_name(&entry.archive_path)
        .map_err(|error| CompressError::Single(error.into()))?;
    let mut bytes = Vec::new();
    source
        .read_to_end(&mut bytes)
        .map_err(|error| CompressError::Single(error.into()))?;
    let data = super::payload::read_registry_payload(&entry.archive_path, &bytes)?;
    crate::backup::registry::import_registry_data(&data, &entry.target_path.to_string_lossy())
        .map_err(|error| CompressError::Single(BackupFileError::RegistryError(error.to_string())))
}

#[cfg(test)]
#[path = "legacy_restore_fixture.rs"]
mod legacy_restore_fixture;
#[cfg(test)]
pub use legacy_restore_fixture::decompress_from_file;

#[cfg(test)]
mod tests {
    use std::fs;

    use filetime::{FileTime, set_file_mtime};

    use super::read_capture_manifest;
    use crate::backup::CaptureSourceKind;

    #[test]
    fn corrupt_zip_payload_cannot_delete_or_overwrite_live_saves() {
        use std::io::Write;
        let temp = temp_dir::TempDir::new().unwrap();
        let archive = temp.path().join("damaged.zip");
        let target = temp.path().join("profile.sav");
        fs::write(&target, b"live-progress").unwrap();
        let mut writer = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        writer
            .start_file(
                "profile.sav",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(b"unique-archived-progress").unwrap();
        writer.finish().unwrap();
        let mut bytes = fs::read(&archive).unwrap();
        let offset = bytes
            .windows(b"unique-archived-progress".len())
            .position(|window| window == b"unique-archived-progress")
            .unwrap();
        bytes[offset] ^= 1;
        fs::write(&archive, bytes).unwrap();
        let plan = crate::backup::RestorePlan {
            entries: vec![crate::backup::RestoreEntry {
                save_unit_id: 1,
                group_id: 0,
                archive_path: "profile.sav".into(),
                target_path: target.clone(),
                kind: CaptureSourceKind::File,
                delete_before_apply: true,
            }],
            ..Default::default()
        };
        assert!(super::restore_capture_plan(&plan, &archive).is_err());
        assert_eq!(fs::read(target).unwrap(), b"live-progress");
    }

    #[test]
    fn v2_entries_form_a_capture_manifest_without_config_state() {
        use std::io::Write;

        let temp = temp_dir::TempDir::new().unwrap();
        let archive = temp.path().join("legacy-v2.zip");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        writer.set_comment("RGSM_ARCHIVE_V2\n{\"version\":2,\"compression\":\"zip\"}");
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("7/Saved/profile.sav", options).unwrap();
        writer.write_all(b"folder").unwrap();
        writer.start_file("8/slot.sav", options).unwrap();
        writer.write_all(b"file").unwrap();
        writer.start_file("9/registry.reg", options).unwrap();
        writer.write_all(b"registry").unwrap();
        writer.finish().unwrap();

        let manifest = read_capture_manifest(&archive).unwrap();

        assert_eq!(manifest.version, 2);
        assert_eq!(manifest.groups.len(), 3);
        assert_eq!(manifest.groups[0].archive_path, "7/Saved");
        assert_eq!(manifest.groups[0].kind, CaptureSourceKind::Directory);
        assert_eq!(manifest.groups[1].kind, CaptureSourceKind::File);
        assert_eq!(manifest.groups[2].kind, CaptureSourceKind::Registry);
    }

    #[test]
    fn v2_manifest_rejects_multiple_roots_for_one_save_unit() {
        use std::io::Write;

        let temp = temp_dir::TempDir::new().unwrap();
        let archive = temp.path().join("ambiguous-v2.zip");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
        writer.set_comment("RGSM_ARCHIVE_V2\n{\"version\":2,\"compression\":\"zip\"}");
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("7/a.sav", options).unwrap();
        writer.write_all(b"a").unwrap();
        writer.start_file("7/b.sav", options).unwrap();
        writer.write_all(b"b").unwrap();
        writer.finish().unwrap();

        assert!(read_capture_manifest(&archive).is_err());
    }

    #[test]
    fn v2_restore_decodes_escaped_literals_before_writing_target() {
        use std::collections::BTreeMap;
        use std::io::Write;

        use crate::backup::{ArchiveBackend, RestorePlan, ZipBackend};
        use crate::path_resolution::{
            CandidateDimensions, CandidateExpression, ResolutionReport, ResolutionSelectionState,
        };

        let temp = temp_dir::TempDir::new().unwrap();
        let archive = temp.path().join("legacy-v2.zip");
        let target = temp.path().join("Games[Main]").join("Saved");
        let mut writer = zip::ZipWriter::new(fs::File::create(&archive).unwrap());
        writer.set_comment("RGSM_ARCHIVE_V2\n{\"version\":2,\"compression\":\"zip\"}");
        writer
            .start_file(
                "7/Saved/profile.sav",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"captured").unwrap();
        writer.finish().unwrap();

        let manifest = read_capture_manifest(&archive).unwrap();
        let expression = globset::escape(&target.to_string_lossy().replace('\\', "/"));
        let reports = BTreeMap::from([(
            7,
            ResolutionReport {
                raw_pattern: "<root>/Saved".to_string(),
                selection_state: ResolutionSelectionState::ImplicitUnique {
                    candidate_id: "root".to_string(),
                },
                candidates: vec![CandidateExpression {
                    id: "root".to_string(),
                    expression,
                    logical_anchor: target.parent().unwrap().to_string_lossy().into_owned(),
                    dimensions: CandidateDimensions::default(),
                    case_sensitive: false,
                }],
                locations: Vec::new(),
                diagnostics: Vec::new(),
            },
        )]);
        let plan = RestorePlan::build_legacy_v2(&manifest.groups, &reports, &[]).unwrap();

        ZipBackend.restore_capture_plan(&plan, &archive).unwrap();

        assert_eq!(fs::read(target.join("profile.sav")).unwrap(), b"captured");
        assert!(!temp.path().join("Games[[]Main[]]").exists());
    }

    #[test]
    fn v3_restore_plan_extracts_only_approved_target() {
        use crate::backup::{
            ArchiveBackend, CaptureGroup, CapturePlan, CaptureSourceKind, CompressionPreset,
            RestoreEntry, RestorePlan, ZipBackend,
        };
        use crate::path_resolution::CandidateDimensions;

        let temp = temp_dir::TempDir::new().unwrap();
        let source = temp.path().join("source.dat");
        let target = temp.path().join("restore").join("target.dat");
        let archive = temp.path().join("snapshot.zip");
        fs::write(&source, b"captured").unwrap();
        let capture = CapturePlan {
            groups: vec![CaptureGroup {
                id: 0,
                save_unit_id: 4,
                candidate_id: "source".to_string(),
                dimensions: CandidateDimensions::default(),
                logical_anchor: temp.path().to_path_buf(),
                source_path: source.to_string_lossy().into_owned(),
                relative_path: "source.dat".to_string(),
                archive_path: "4/0/data/source.dat".to_string(),
                kind: CaptureSourceKind::File,
                delete_before_apply: true,
            }],
        };
        ZipBackend
            .compress_capture_plan(&capture, &archive, CompressionPreset::Standard, None)
            .unwrap();
        let restore = RestorePlan {
            entries: vec![RestoreEntry {
                save_unit_id: 4,
                group_id: 0,
                archive_path: "4/0/data/source.dat".to_string(),
                target_path: target.clone(),
                kind: CaptureSourceKind::File,
                delete_before_apply: true,
            }],
            skipped_inactive_save_unit_ids: Vec::new(),
        };

        ZipBackend.restore_capture_plan(&restore, &archive).unwrap();

        assert_eq!(fs::read(target).unwrap(), b"captured");
        assert_eq!(fs::read(source).unwrap(), b"captured");
    }

    #[test]
    fn v3_restore_verifies_entries_before_deleting_existing_target() {
        use crate::backup::{CaptureSourceKind, RestoreEntry, RestorePlan};

        let temp = temp_dir::TempDir::new().unwrap();
        let archive = temp.path().join("empty.zip");
        let target = temp.path().join("save.dat");
        fs::write(&target, b"keep").unwrap();
        zip::ZipWriter::new(fs::File::create(&archive).unwrap())
            .finish()
            .unwrap();
        let restore = RestorePlan {
            entries: vec![RestoreEntry {
                save_unit_id: 1,
                group_id: 0,
                archive_path: "1/0/data/save.dat".to_string(),
                target_path: target.clone(),
                kind: CaptureSourceKind::File,
                delete_before_apply: true,
            }],
            skipped_inactive_save_unit_ids: Vec::new(),
        };

        assert!(super::restore_capture_plan(&restore, &archive).is_err());
        assert_eq!(fs::read(target).unwrap(), b"keep");
    }

    #[test]
    fn v3_restore_reapplies_nested_directory_mtimes_after_children() {
        use crate::backup::{
            ArchiveBackend, CaptureGroup, CapturePlan, CaptureSourceKind, CompressionPreset,
            RestoreEntry, RestorePlan, ZipBackend,
        };
        use crate::path_resolution::CandidateDimensions;

        let temp = temp_dir::TempDir::new().unwrap();
        let source = temp.path().join("source");
        let nested = source.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("save.dat"), b"save").unwrap();
        let source_time = FileTime::from_unix_time(1_704_164_644, 0);
        let nested_time = FileTime::from_unix_time(1_704_251_044, 0);
        set_file_mtime(&nested, nested_time).unwrap();
        set_file_mtime(&source, source_time).unwrap();
        let archive = temp.path().join("snapshot.zip");
        let capture = CapturePlan {
            groups: vec![CaptureGroup {
                id: 0,
                save_unit_id: 1,
                candidate_id: "source".into(),
                dimensions: CandidateDimensions::default(),
                logical_anchor: temp.path().to_path_buf(),
                source_path: source.to_string_lossy().into_owned(),
                relative_path: "source".into(),
                archive_path: "1/0/data/source".into(),
                kind: CaptureSourceKind::Directory,
                delete_before_apply: false,
            }],
        };
        ZipBackend
            .compress_capture_plan(&capture, &archive, CompressionPreset::Fast, None)
            .unwrap();
        let target = temp.path().join("restored");
        ZipBackend
            .restore_capture_plan(
                &RestorePlan {
                    entries: vec![RestoreEntry {
                        save_unit_id: 1,
                        group_id: 0,
                        archive_path: "1/0/data/source".into(),
                        target_path: target.clone(),
                        kind: CaptureSourceKind::Directory,
                        delete_before_apply: false,
                    }],
                    skipped_inactive_save_unit_ids: Vec::new(),
                },
                &archive,
            )
            .unwrap();

        assert_eq!(
            FileTime::from_last_modification_time(&fs::metadata(&target).unwrap()),
            source_time
        );
        assert_eq!(
            FileTime::from_last_modification_time(&fs::metadata(target.join("nested")).unwrap()),
            nested_time
        );
    }
}
