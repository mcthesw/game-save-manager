use std::{fs, path::Path};

use crate::{
    backup::{CaptureGroup, CapturePlan, CaptureSourceKind, CompressionPreset, CreatedBy},
    path_resolution::CandidateDimensions,
};

use super::{
    ArchiveIdentity,
    portable::{ArchiveLocation, prepare_archive},
    seven_z::{read_manifest, write_snapshot},
};

fn identity() -> ArchiveIdentity {
    ArchiveIdentity {
        recovered_metadata: false,
        game_id: "game".into(),
        game_name: "Game".into(),
        snapshot_id: "snapshot".into(),
        created_at: None,
        legacy_local_time: None,
        device_id: None,
        parent: None,
        description: String::new(),
        created_by: CreatedBy::Manual,
        locations: (0..10)
            .map(|id| ArchiveLocation {
                save_unit_id: id,
                expression: "<base>/Saves".into(),
            })
            .collect(),
    }
}

#[test]
fn recovery_document_uses_plain_english_and_preserves_player_text() {
    let plan = CapturePlan {
        groups: vec![group(
            0,
            0,
            "C:/玩家/profile.sav",
            "",
            CaptureSourceKind::File,
        )],
    };
    let mut identity = identity();
    identity.game_name = "星海旅人".into();
    identity.description = "最终战之前".into();
    let (_, manifest) = prepare_archive(&plan, identity, None).unwrap();
    let text = super::portable::recovery_instructions(&manifest);
    assert!(text.starts_with("Game Save Manager backup\n"));
    assert!(text.contains("Game: 星海旅人\n"));
    assert!(text.contains("Note: 最终战之前\n"));
    assert!(text.contains("Original path: C:/玩家/profile.sav"));
    assert!(text.contains("_rgsm/manifest.json"));
    assert!(!text.contains("Snapshot ID:"));
}

fn group(
    unit: u32,
    id: u32,
    anchor: &str,
    relative: &str,
    kind: CaptureSourceKind,
) -> CaptureGroup {
    CaptureGroup {
        id,
        save_unit_id: unit,
        logical_anchor: anchor.into(),
        source_path: if relative.is_empty() {
            anchor.into()
        } else {
            format!("{anchor}/{relative}")
        },
        relative_path: relative.into(),
        relative_expression: None,
        archive_path: String::new(),
        candidate_id: "resource:0".into(),
        dimensions: CandidateDimensions::default(),
        kind,
        delete_before_apply: false,
    }
}

#[test]
fn readable_layout_preserves_wildcard_subtrees_and_disambiguates_units() {
    let plan = CapturePlan {
        groups: vec![
            group(
                0,
                0,
                "C:/account/Saves",
                "one/save.dat",
                CaptureSourceKind::File,
            ),
            group(
                0,
                1,
                "C:/account/Saves",
                "two/save.dat",
                CaptureSourceKind::File,
            ),
            group(1, 2, "D:/game/Saves", "", CaptureSourceKind::Directory),
            group(2, 3, "C:/game/RESTORE.txt", "", CaptureSourceKind::File),
            group(
                3,
                4,
                "HKEY_CURRENT_USER\\Software\\My Game",
                "",
                CaptureSourceKind::Registry,
            ),
        ],
    };
    let (_, manifest) = prepare_archive(&plan, identity(), None).unwrap();
    assert_eq!(
        manifest
            .groups
            .iter()
            .map(|g| g.archive_path.as_str())
            .collect::<Vec<_>>(),
        [
            "Saves/one/save.dat",
            "Saves/two/save.dat",
            "Saves (2)",
            "RESTORE (2).txt",
            "My Game.reg"
        ]
    );
    assert!(manifest.groups.iter().all(|g| g.candidate_id.is_empty()));
    assert_eq!(manifest.identity.unwrap().created_at, None);
}

#[test]
fn recursive_glob_captures_a_directory_and_its_children_only_once() {
    let plan = CapturePlan {
        groups: vec![
            group(0, 0, "C:/game/Saves", "slot", CaptureSourceKind::Directory),
            group(
                0,
                1,
                "C:/game/Saves",
                "slot/save.dat",
                CaptureSourceKind::File,
            ),
            group(0, 2, "C:/game/Saves", "other.dat", CaptureSourceKind::File),
        ],
    };
    let (plan, manifest) = prepare_archive(&plan, identity(), None).unwrap();
    assert_eq!(plan.groups.len(), 2);
    assert_eq!(manifest.groups.len(), 2);
}

#[test]
fn portable_layout_rejects_unsafe_or_colliding_names() {
    for relative in ["../outside.dat", "/outside.dat", "C:/outside.dat"] {
        let plan = CapturePlan {
            groups: vec![group(0, 0, "C:/Saves", relative, CaptureSourceKind::File)],
        };
        assert!(prepare_archive(&plan, identity(), None).is_err());
    }
    let plan = CapturePlan {
        groups: vec![
            group(0, 0, "C:/Saves", "save.dat", CaptureSourceKind::File),
            group(0, 1, "C:/Saves", "SAVE.DAT", CaptureSourceKind::File),
        ],
    };
    assert!(prepare_archive(&plan, identity(), None).is_err());
}

#[test]
fn ordinary_extraction_preserves_file_and_folder_metadata() {
    let temp = temp_dir::TempDir::new().unwrap();
    let source = temp.path().join("Saves");
    fs::create_dir_all(source.join("Empty")).unwrap();
    fs::write(source.join("slot.sav"), b"save-data").unwrap();
    let mtime = filetime::FileTime::from_unix_time(1_704_164_645, 0);
    filetime::set_file_mtime(source.join("slot.sav"), mtime).unwrap();
    let plan = CapturePlan {
        groups: vec![group(
            0,
            0,
            &source.to_string_lossy(),
            "",
            CaptureSourceKind::Directory,
        )],
    };
    let path = temp.path().join("backup.7z");
    write_snapshot(&plan, &path, CompressionPreset::Standard, identity(), None).unwrap();
    let extracted = temp.path().join("extracted");
    sevenz_rust2::decompress_file(&path, &extracted).unwrap();
    assert_eq!(
        fs::read(extracted.join("Saves/slot.sav")).unwrap(),
        b"save-data"
    );
    assert!(extracted.join("Saves/Empty").is_dir());
    assert_eq!(
        filetime::FileTime::from_last_modification_time(
            &fs::metadata(extracted.join("Saves/slot.sav")).unwrap()
        ),
        mtime
    );
    assert_eq!(read_manifest(&path).unwrap().version, 6);
}

#[test]
fn duplicate_or_anonymous_current_manifests_are_rejected() {
    for duplicate in [false, true] {
        let temp = temp_dir::TempDir::new().unwrap();
        let path = temp.path().join("invalid.7z");
        let mut writer = sevenz_rust2::ArchiveWriter::create(&path).unwrap();
        let bytes = br#"{"version":6,"groups":[]}"#;
        for _ in 0..if duplicate { 2 } else { 1 } {
            writer
                .push_archive_entry(
                    sevenz_rust2::ArchiveEntry::new_file("_rgsm/manifest.json"),
                    Some(std::io::Cursor::new(bytes)),
                )
                .unwrap();
        }
        writer.finish().unwrap();
        assert!(read_manifest(Path::new(&path)).is_err());
    }
}
