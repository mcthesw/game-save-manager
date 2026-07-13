use std::{fs, io::Cursor, path::Path};

use sevenz_rust2::{ArchiveEntry, ArchiveReader, ArchiveWriter, EncoderMethod, Password};

use crate::{
    backup::{
        CaptureGroup, CapturePlan, CaptureSourceKind, CompressionPreset, RestoreEntry, RestorePlan,
    },
    path_resolution::CandidateDimensions,
};

use super::seven_z::{compress_capture_plan, read_manifest, restore_capture_plan};

fn capture_plan(source: &Path, kind: CaptureSourceKind) -> CapturePlan {
    CapturePlan {
        groups: vec![CaptureGroup {
            id: 0,
            save_unit_id: 7,
            candidate_id: "source".into(),
            dimensions: CandidateDimensions::default(),
            logical_anchor: source.parent().unwrap().to_path_buf(),
            source_path: source.to_string_lossy().into_owned(),
            relative_path: "save.dat".into(),
            archive_path: "7/0/data/save.dat".into(),
            kind,
            delete_before_apply: false,
        }],
    }
}

fn restore_plan(target: &Path, kind: CaptureSourceKind) -> RestorePlan {
    RestorePlan {
        entries: vec![RestoreEntry {
            save_unit_id: 7,
            group_id: 0,
            archive_path: "7/0/data/save.dat".into(),
            target_path: target.to_path_buf(),
            kind,
            delete_before_apply: false,
        }],
    }
}

#[test]
fn v4_round_trip_preserves_file_content_mtime_and_manifest() {
    let temp = temp_dir::TempDir::new().unwrap();
    let source = temp.path().join("source.dat");
    fs::write(&source, b"save-data").unwrap();
    let expected_atime = filetime::FileTime::from_unix_time(1_704_164_535, 0);
    let expected_mtime = filetime::FileTime::from_unix_time(1_704_164_645, 0);
    filetime::set_file_times(&source, expected_atime, expected_mtime).unwrap();
    #[cfg(windows)]
    let expected_created = fs::metadata(&source).unwrap().created().ok();
    let archive = temp.path().join("snapshot.7z");

    compress_capture_plan(
        &capture_plan(&source, CaptureSourceKind::File),
        &archive,
        CompressionPreset::Standard,
        Some("fingerprint".into()),
    )
    .unwrap();
    assert_eq!(
        filetime::FileTime::from_last_access_time(&fs::metadata(&source).unwrap()),
        expected_atime,
        "capturing an archive must not mutate source atime"
    );
    let manifest = read_manifest(&archive).unwrap();
    assert_eq!(manifest.version, 4);
    assert_eq!(manifest.source_fingerprint.as_deref(), Some("fingerprint"));

    let target = temp.path().join("restore/save.dat");
    restore_capture_plan(&restore_plan(&target, CaptureSourceKind::File), &archive).unwrap();

    let restored_metadata = fs::metadata(&target).unwrap();
    assert_eq!(
        filetime::FileTime::from_last_access_time(&restored_metadata),
        expected_atime
    );
    #[cfg(windows)]
    if let Some(expected_created) = expected_created {
        assert_eq!(restored_metadata.created().unwrap(), expected_created);
    }
    assert_eq!(fs::read(&target).unwrap(), b"save-data");
    assert_eq!(
        filetime::FileTime::from_last_modification_time(&fs::metadata(target).unwrap()),
        expected_mtime
    );
}

#[test]
fn v4_round_trip_reapplies_nested_directory_times_after_children() {
    let temp = temp_dir::TempDir::new().unwrap();
    let source = temp.path().join("source");
    let nested = source.join("nested");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("save.dat"), b"save-data").unwrap();
    let root_mtime = filetime::FileTime::from_unix_time(1_704_164_645, 0);
    let nested_mtime = filetime::FileTime::from_unix_time(1_704_164_755, 0);
    filetime::set_file_mtime(&source, root_mtime).unwrap();
    filetime::set_file_mtime(&nested, nested_mtime).unwrap();
    let archive = temp.path().join("snapshot.7z");
    compress_capture_plan(
        &capture_plan(&source, CaptureSourceKind::Directory),
        &archive,
        CompressionPreset::Standard,
        None,
    )
    .unwrap();

    let target = temp.path().join("restore");
    restore_capture_plan(
        &restore_plan(&target, CaptureSourceKind::Directory),
        &archive,
    )
    .unwrap();

    assert_eq!(
        fs::read(target.join("nested/save.dat")).unwrap(),
        b"save-data"
    );
    assert_eq!(
        filetime::FileTime::from_last_modification_time(&fs::metadata(&target).unwrap()),
        root_mtime
    );
    assert_eq!(
        filetime::FileTime::from_last_modification_time(
            &fs::metadata(target.join("nested")).unwrap()
        ),
        nested_mtime
    );
}

#[test]
fn v4_round_trip_preserves_empty_files() {
    let temp = temp_dir::TempDir::new().unwrap();
    let source = temp.path().join("empty.dat");
    fs::write(&source, []).unwrap();
    let archive = temp.path().join("empty.7z");
    compress_capture_plan(
        &capture_plan(&source, CaptureSourceKind::File),
        &archive,
        CompressionPreset::Standard,
        None,
    )
    .unwrap();
    let target = temp.path().join("restore/empty.dat");
    restore_capture_plan(&restore_plan(&target, CaptureSourceKind::File), &archive).unwrap();
    assert_eq!(fs::metadata(target).unwrap().len(), 0);
}

#[test]
fn v4_restore_applies_one_archive_entry_to_every_mapped_target() {
    let temp = temp_dir::TempDir::new().unwrap();
    let source = temp.path().join("source.dat");
    fs::write(&source, b"save-data").unwrap();
    let archive = temp.path().join("snapshot.7z");
    compress_capture_plan(
        &capture_plan(&source, CaptureSourceKind::File),
        &archive,
        CompressionPreset::Standard,
        None,
    )
    .unwrap();

    let first = temp.path().join("restore-a/save.dat");
    let second = temp.path().join("restore-b/save.dat");
    fs::create_dir_all(first.parent().unwrap()).unwrap();
    fs::create_dir_all(second.parent().unwrap()).unwrap();
    fs::write(&first, b"old-a").unwrap();
    fs::write(&second, b"old-b").unwrap();
    let mut plan = restore_plan(&first, CaptureSourceKind::File);
    plan.entries[0].delete_before_apply = true;
    plan.entries.push(RestoreEntry {
        target_path: second.clone(),
        ..plan.entries[0].clone()
    });

    restore_capture_plan(&plan, &archive).unwrap();

    assert_eq!(fs::read(first).unwrap(), b"save-data");
    assert_eq!(fs::read(second).unwrap(), b"save-data");
}

#[cfg(windows)]
#[test]
fn v4_round_trip_preserves_settable_windows_attributes() {
    use std::os::windows::{ffi::OsStrExt, fs::MetadataExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_READONLY,
        FILE_ATTRIBUTE_SYSTEM, SetFileAttributesW,
    };

    fn set_attributes(path: &Path, attributes: u32) {
        let wide = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        assert_ne!(unsafe { SetFileAttributesW(wide.as_ptr(), attributes) }, 0);
    }

    let temp = temp_dir::TempDir::new().unwrap();
    let source = temp.path().join("source.dat");
    fs::write(&source, b"save-data").unwrap();
    let expected = FILE_ATTRIBUTE_READONLY | FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM;
    set_attributes(&source, expected);
    let archive = temp.path().join("snapshot.7z");
    compress_capture_plan(
        &capture_plan(&source, CaptureSourceKind::File),
        &archive,
        CompressionPreset::Standard,
        None,
    )
    .unwrap();

    let target = temp.path().join("restore/save.dat");
    restore_capture_plan(&restore_plan(&target, CaptureSourceKind::File), &archive).unwrap();

    let actual = fs::metadata(&target).unwrap().file_attributes();
    assert_eq!(actual & expected, expected);
    set_attributes(&source, FILE_ATTRIBUTE_NORMAL);
    set_attributes(&target, FILE_ATTRIBUTE_NORMAL);
}

#[cfg(unix)]
#[test]
fn v4_round_trip_preserves_full_posix_modes() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    for mode in [0o4751, 0o2750, 0o1770] {
        let temp = temp_dir::TempDir::new().unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        let source_file = source.join("save.dat");
        fs::write(&source_file, b"save-data").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(mode)).unwrap();
        fs::set_permissions(&source_file, fs::Permissions::from_mode(mode)).unwrap();
        let archive = temp.path().join("snapshot.7z");
        compress_capture_plan(
            &capture_plan(&source, CaptureSourceKind::Directory),
            &archive,
            CompressionPreset::Store,
            None,
        )
        .unwrap();

        let target = temp.path().join("restore");
        restore_capture_plan(
            &restore_plan(&target, CaptureSourceKind::Directory),
            &archive,
        )
        .unwrap();
        assert_eq!(fs::metadata(target).unwrap().mode() & 0o7777, mode);
        assert_eq!(
            fs::metadata(temp.path().join("restore/save.dat"))
                .unwrap()
                .mode()
                & 0o7777,
            mode
        );
    }
}

#[test]
fn v4_archives_are_non_solid_for_every_preset() {
    let temp = temp_dir::TempDir::new().unwrap();
    let source = temp.path().join("source.dat");
    fs::write(&source, b"save-data").unwrap();
    for preset in [
        CompressionPreset::Store,
        CompressionPreset::Fast,
        CompressionPreset::Standard,
        CompressionPreset::Best,
    ] {
        let archive = temp.path().join(format!("{preset:?}.7z"));
        compress_capture_plan(
            &capture_plan(&source, CaptureSourceKind::File),
            &archive,
            preset,
            None,
        )
        .unwrap();
        let reader = ArchiveReader::open(&archive, Password::empty()).unwrap();
        assert!(!reader.archive().is_solid, "{preset:?} must be non-solid");
    }
}

#[test]
fn missing_planned_entry_is_rejected_before_target_deletion() {
    let temp = temp_dir::TempDir::new().unwrap();
    let source = temp.path().join("source.dat");
    fs::write(&source, b"save-data").unwrap();
    let archive = temp.path().join("snapshot.7z");
    compress_capture_plan(
        &capture_plan(&source, CaptureSourceKind::File),
        &archive,
        CompressionPreset::Fast,
        None,
    )
    .unwrap();
    let target = temp.path().join("existing.dat");
    fs::write(&target, b"keep-me").unwrap();
    let plan = RestorePlan {
        entries: vec![RestoreEntry {
            save_unit_id: 7,
            group_id: 0,
            archive_path: "missing.dat".into(),
            target_path: target.clone(),
            kind: CaptureSourceKind::File,
            delete_before_apply: true,
        }],
    };

    assert!(restore_capture_plan(&plan, &archive).is_err());
    assert_eq!(fs::read(target).unwrap(), b"keep-me");
}

#[test]
fn unsafe_directory_entry_is_rejected_before_target_deletion() {
    let temp = temp_dir::TempDir::new().unwrap();
    let archive = temp.path().join("unsafe.7z");
    let mut writer = ArchiveWriter::create(&archive).unwrap();
    writer.set_content_methods(vec![EncoderMethod::COPY.into()]);
    writer
        .push_archive_entry::<&[u8]>(ArchiveEntry::new_directory("7/0/data/save.dat"), None)
        .unwrap();
    writer
        .push_archive_entry(
            ArchiveEntry::new_file("7/0/data/save.dat/../../escape.dat"),
            Some(Cursor::new(b"unsafe")),
        )
        .unwrap();
    writer.finish().unwrap();

    let target = temp.path().join("existing");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep.dat"), b"keep-me").unwrap();
    let mut plan = restore_plan(&target, CaptureSourceKind::Directory);
    plan.entries[0].delete_before_apply = true;

    assert!(restore_capture_plan(&plan, &archive).is_err());
    assert_eq!(fs::read(target.join("keep.dat")).unwrap(), b"keep-me");
}

#[test]
fn duplicate_entry_is_rejected_before_target_deletion() {
    let temp = temp_dir::TempDir::new().unwrap();
    let archive = temp.path().join("duplicate.7z");
    let mut writer = ArchiveWriter::create(&archive).unwrap();
    writer.set_content_methods(vec![EncoderMethod::COPY.into()]);
    for content in [b"first".as_slice(), b"second".as_slice()] {
        writer
            .push_archive_entry(
                ArchiveEntry::new_file("7/0/data/save.dat"),
                Some(Cursor::new(content)),
            )
            .unwrap();
    }
    writer.finish().unwrap();
    let target = temp.path().join("existing.dat");
    fs::write(&target, b"keep-me").unwrap();
    let mut plan = restore_plan(&target, CaptureSourceKind::File);
    plan.entries[0].delete_before_apply = true;

    assert!(restore_capture_plan(&plan, &archive).is_err());
    assert_eq!(fs::read(target).unwrap(), b"keep-me");
}

#[cfg(windows)]
#[test]
fn official_7zip_reads_and_extracts_v4_with_mtime() {
    use std::process::Command;

    let available = Command::new("7z").arg("i").output();
    let Ok(available) = available else {
        return;
    };
    if !available.status.success() {
        return;
    }

    let temp = temp_dir::TempDir::new().unwrap();
    let source = temp.path().join("source.dat");
    fs::write(&source, b"save-data").unwrap();
    let expected_mtime = filetime::FileTime::from_unix_time(1_704_164_645, 0);
    filetime::set_file_mtime(&source, expected_mtime).unwrap();
    let archive = temp.path().join("snapshot.7z");
    compress_capture_plan(
        &capture_plan(&source, CaptureSourceKind::File),
        &archive,
        CompressionPreset::Standard,
        None,
    )
    .unwrap();

    let tested = Command::new("7z").arg("t").arg(&archive).output().unwrap();
    assert!(
        tested.status.success(),
        "official 7-Zip rejected V4: {}",
        String::from_utf8_lossy(&tested.stderr)
    );

    let extracted = temp.path().join("official-extract");
    let output_switch = format!("-o{}", extracted.display());
    let output = Command::new("7z")
        .args(["x", "-y", &output_switch])
        .arg(&archive)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "official 7-Zip could not extract V4: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let restored = extracted.join("7/0/data/save.dat");
    assert_eq!(fs::read(&restored).unwrap(), b"save-data");
    assert_eq!(
        filetime::FileTime::from_last_modification_time(&fs::metadata(restored).unwrap()),
        expected_mtime
    );
}

#[cfg(windows)]
#[test]
fn official_7zip_archive_is_accepted_by_v4_apply() {
    use std::process::Command;

    if !Command::new("7z")
        .arg("i")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return;
    }

    let temp = temp_dir::TempDir::new().unwrap();
    let fixture = temp.path().join("fixture");
    fs::create_dir_all(fixture.join("7/0/data")).unwrap();
    fs::write(fixture.join("7/0/data/save.dat"), b"official-save").unwrap();
    let source = fixture.join("7/0/data/save.dat");
    let manifest =
        super::ArchiveManifestV4::from_plan(&capture_plan(&source, CaptureSourceKind::File), None);
    fs::create_dir_all(fixture.join("_rgsm")).unwrap();
    fs::write(
        fixture.join("_rgsm/manifest-v4.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let archive = temp.path().join("official.7z");
    let output = Command::new("7z")
        .current_dir(&fixture)
        .args(["a", "-t7z", "-m0=Deflate", "-mx=1", "-ms=off"])
        .arg(&archive)
        .args(["7", "_rgsm"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "official fixture creation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(read_manifest(&archive).unwrap().version, 4);

    let target = temp.path().join("restore/save.dat");
    restore_capture_plan(&restore_plan(&target, CaptureSourceKind::File), &archive).unwrap();
    assert_eq!(fs::read(target).unwrap(), b"official-save");
}
