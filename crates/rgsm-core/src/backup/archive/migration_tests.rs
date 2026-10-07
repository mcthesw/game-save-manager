use std::{collections::BTreeMap, fs, io::Write, path::Path};

use crate::backup::{
    ArchiveFormat, CaptureGroup, CapturePlan, CaptureSourceKind, CompressionPreset, Game, Snapshot,
};

use super::{
    ArchiveBackend, ArchiveMeta, SevenZBackend, ZipBackend,
    migration::{ArchiveMigrationError, ArchiveMigrationInput, convert_snapshot_archive},
};

fn game() -> Game {
    serde_json::from_value(serde_json::json!({
        "name": "Traveller", "storage_key": "traveller", "save_paths": [{"id":7,
        "source":{"type":"devicePaths", "unit_type":"File", "paths":{"old-device":"X:/Uninstalled/profile.sav"}}}]
    })).unwrap()
}

fn snapshot(path: &Path, format: ArchiveFormat) -> Snapshot {
    serde_json::from_value(serde_json::json!({
        "date":"2020-01-02_03-04-05", "describe":"Old progress", "parent":"earlier", "path":path,
        "archive_format":format, "device_id":"old-device"
    }))
    .unwrap()
}

fn zip(path: &Path, comment: &str, name: &str, bytes: &[u8]) {
    let mut writer = zip::ZipWriter::new(fs::File::create(path).unwrap());
    writer.set_comment(comment);
    writer
        .start_file(name, zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(bytes).unwrap();
    writer.finish().unwrap();
}

fn capture(path: &Path) -> CapturePlan {
    CapturePlan {
        groups: vec![CaptureGroup {
            id: 0,
            save_unit_id: 7,
            candidate_id: "source".into(),
            dimensions: Default::default(),
            relative_expression: None,
            logical_anchor: path.into(),
            source_path: path.to_string_lossy().into_owned(),
            relative_path: String::new(),
            archive_path: "7/0/data/profile.sav".into(),
            kind: CaptureSourceKind::File,
            delete_before_apply: false,
        }],
    }
}

fn convert(
    game: &Game,
    source: &Path,
    destination: &Path,
    format: ArchiveFormat,
    associations: &BTreeMap<String, u32>,
) -> Result<u64, ArchiveMigrationError> {
    convert_snapshot_archive(ArchiveMigrationInput {
        game,
        snapshot: &snapshot(source, format),
        source,
        destination,
        device_id: &"this-device".into(),
        preset: CompressionPreset::Standard,
        associations,
    })
}

#[test]
fn every_historical_archive_generation_converts_without_live_game_paths() {
    let temp = temp_dir::TempDir::new().unwrap();
    let source_save = temp.path().join("Saves/slot/old/profile.sav");
    fs::create_dir_all(source_save.parent().unwrap()).unwrap();
    fs::write(&source_save, b"old-progress").unwrap();
    for version in 0..=5 {
        let format = if version >= 4 {
            ArchiveFormat::SevenZ
        } else {
            ArchiveFormat::Zip
        };
        let old = temp
            .path()
            .join(format!("v{version}.{}", format.extension()));
        let new = temp.path().join(format!("converted-{version}.7z"));
        match version {
            0 => zip(&old, "", "profile.sav", b"old-progress"),
            1 => zip(
                &old,
                super::V1_COMMENT_MARKER,
                "profile.sav",
                b"old-progress",
            ),
            2 => zip(
                &old,
                &ArchiveMeta::new(CompressionPreset::Standard).to_comment(),
                "7/profile.sav",
                b"old-progress",
            ),
            3 => {
                ZipBackend
                    .compress_capture_plan(
                        &capture(&source_save),
                        &old,
                        CompressionPreset::Standard,
                        None,
                    )
                    .unwrap();
            }
            4 => {
                SevenZBackend
                    .compress_capture_plan(
                        &capture(&source_save),
                        &old,
                        CompressionPreset::Standard,
                        None,
                    )
                    .unwrap();
            }
            _ => {
                let mut plan = capture(&source_save);
                plan.groups[0].relative_path = "slot/old/profile.sav".into();
                plan.groups[0].relative_expression = Some("slot/<var:account>/profile.sav".into());
                plan.groups[0].logical_anchor = temp.path().join("Saves");
                SevenZBackend
                    .compress_capture_plan(&plan, &old, CompressionPreset::Standard, None)
                    .unwrap();
            }
        }
        let original = fs::read(&old).unwrap();
        convert(&game(), &old, &new, format, &BTreeMap::new()).unwrap();
        assert_eq!(fs::read(&old).unwrap(), original);
        let manifest = SevenZBackend.read_capture_manifest(&new).unwrap();
        let identity = manifest.identity.unwrap();
        assert_eq!(manifest.version, 6);
        assert_eq!(identity.snapshot_id, "2020-01-02_03-04-05");
        assert_eq!(identity.parent.as_deref(), Some("earlier"));
        assert!(identity.created_at.is_none());
        assert_eq!(
            identity.legacy_local_time.as_deref(),
            Some("2020-01-02_03-04-05")
        );
        assert_eq!(identity.device_id.as_deref(), Some("old-device"));
        let output = temp.path().join(format!("extracted-{version}"));
        sevenz_rust2::decompress_file(&new, &output).unwrap();
        assert_eq!(
            fs::read(output.join(&manifest.groups[0].archive_path)).unwrap(),
            b"old-progress"
        );
        if version == 5 {
            assert_eq!(
                manifest.groups[0].relative_expression.as_deref(),
                Some("slot/<var:account>/profile.sav")
            );
        }
    }
}

#[test]
fn ambiguous_flat_archives_wait_for_an_explicit_association() {
    let temp = temp_dir::TempDir::new().unwrap();
    let old = temp.path().join("old.zip");
    let new = temp.path().join("new.7z");
    zip(&old, "", "profile.sav", b"save");
    let mut game = game();
    let mut other = game.save_paths[0].clone();
    other.id = 9;
    game.save_paths.push(other);
    assert!(matches!(
        convert(&game, &old, &new, ArchiveFormat::Zip, &BTreeMap::new()),
        Err(ArchiveMigrationError::Association(_))
    ));
    assert!(!new.exists());
    convert(
        &game,
        &old,
        &new,
        ArchiveFormat::Zip,
        &BTreeMap::from([("profile.sav".into(), 9)]),
    )
    .unwrap();
    assert_eq!(
        SevenZBackend.read_capture_manifest(&new).unwrap().groups[0].save_unit_id,
        9
    );
}

#[test]
fn registry_json_is_converted_to_an_ordinary_reg_file_without_importing_it() {
    registry_migration_roundtrip(false);
}

#[test]
fn registry_migration_preserves_reg_precedence_and_the_original_json_fallback() {
    registry_migration_roundtrip(true);
}

fn registry_migration_roundtrip(with_reg: bool) {
    let temp = temp_dir::TempDir::new().unwrap();
    let old = temp.path().join("registry.zip");
    let new = temp.path().join("registry.7z");
    let registry = crate::backup::registry::RegistryData {
        format_version: 1,
        root_key: "HKEY_CURRENT_USER\\Software\\MigrationExample".into(),
        entries: vec![crate::backup::registry::RegistryKeyEntry {
            subkey: String::new(),
            values: vec![crate::backup::registry::RegistryValue::Dword {
                name: "level".into(),
                data: 5,
            }],
        }],
    };
    zip(
        &old,
        &ArchiveMeta::new(CompressionPreset::Standard).to_comment(),
        "7/registry.json",
        &serde_json::to_vec(&registry).unwrap(),
    );
    if with_reg {
        let mut fallback = registry.clone();
        fallback.entries[0].values.clear();
        let mut writer = zip::ZipWriter::new(fs::File::create(&old).unwrap());
        writer.set_comment(ArchiveMeta::new(CompressionPreset::Standard).to_comment());
        writer
            .start_file("7/registry.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer
            .write_all(&serde_json::to_vec(&fallback).unwrap())
            .unwrap();
        writer
            .start_file("7/registry.reg", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer
            .write_all(&crate::backup::registry::serialize_reg_file(&registry).unwrap())
            .unwrap();
        writer.finish().unwrap();
    }
    let original = fs::read(&old).unwrap();
    let mut game = game();
    game.save_paths[0] = crate::backup::SaveUnit::concrete(
        7,
        crate::backup::SaveUnitType::WinRegistry,
        [("old-device".into(), registry.root_key.clone())]
            .into_iter()
            .collect(),
        false,
        true,
    );
    convert(&game, &old, &new, ArchiveFormat::Zip, &BTreeMap::new()).unwrap();
    assert_eq!(fs::read(&old).unwrap(), original);
    let output = temp.path().join("files");
    sevenz_rust2::decompress_file(&new, &output).unwrap();
    let reg = fs::read(output.join("MigrationExample.reg")).unwrap();
    assert!(reg.starts_with(&[0xff, 0xfe]));
    assert_eq!(
        crate::backup::registry::deserialize_reg_file(&reg).unwrap(),
        registry
    );
}

#[test]
fn damaged_unsafe_and_multiple_instance_archives_keep_their_originals() {
    let temp = temp_dir::TempDir::new().unwrap();
    let new = temp.path().join("new.7z");
    let old = temp.path().join("unsafe.zip");
    zip(&old, "", "../outside.sav", b"unsafe");
    assert!(convert(&game(), &old, &new, ArchiveFormat::Zip, &BTreeMap::new()).is_err());
    assert!(!temp.path().parent().unwrap().join("outside.sav").exists());
    fs::write(&old, b"damaged").unwrap();
    assert!(convert(&game(), &old, &new, ArchiveFormat::Zip, &BTreeMap::new()).is_err());
    assert_eq!(fs::read(&old).unwrap(), b"damaged");
    let save = temp.path().join("profile.sav");
    fs::write(&save, b"save").unwrap();
    let mut plan = capture(&save);
    plan.groups[0].dimensions.root_id = Some("library-a".into());
    let mut second = plan.groups[0].clone();
    second.id = 1;
    second.archive_path = "7/1/data/profile.sav".into();
    second.dimensions.root_id = Some("library-b".into());
    plan.groups.push(second);
    let old = temp.path().join("instances.7z");
    SevenZBackend
        .compress_capture_plan(&plan, &old, CompressionPreset::Standard, None)
        .unwrap();
    assert!(matches!(
        convert(&game(), &old, &new, ArchiveFormat::SevenZ, &BTreeMap::new()),
        Err(ArchiveMigrationError::MultipleInstances(7))
    ));
    assert!(old.is_file());
    assert!(!new.exists());
}
