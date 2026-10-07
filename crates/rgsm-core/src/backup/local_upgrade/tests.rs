use std::{fs, io::Write, path::Path};

use crate::backup::Snapshot;

use super::*;

fn fixture() -> (temp_dir::TempDir, Game) {
    let root = temp_dir::TempDir::new().unwrap();
    let game: Game = serde_json::from_value(serde_json::json!({
        "name":"Traveller", "storage_key":"traveller", "save_paths":[{"id":7,
        "source":{"type":"devicePaths","unit_type":"File","paths":{"device":"X:/Uninstalled/profile.sav"}}}]
    })).unwrap();
    fs::create_dir_all(root.path().join("traveller")).unwrap();
    (root, game)
}

fn engine(root: &Path, game: &Game) -> LocalUpgrade {
    LocalUpgrade::new(
        root.into(),
        "device".into(),
        vec![game.clone()],
        CompressionPreset::Standard,
    )
}

fn write_zip(path: &Path) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    zip.start_file("profile.sav", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"progress").unwrap();
    zip.finish().unwrap();
}

fn add_snapshot(root: &Path, game: &Game, id: &str, parent: Option<&str>, present: bool) {
    let path = root.join("traveller").join(format!("{id}.zip"));
    if present {
        write_zip(&path);
    }
    let snapshot: Snapshot = serde_json::from_value(
        serde_json::json!({"date":id,"describe":"unchanged note","path":path,"parent":parent}),
    )
    .unwrap();
    SnapshotCatalog::new(root, game, &"device".into())
        .update::<BackupError>(|catalog| {
            catalog.backups.push(snapshot);
            catalog.set_head_for_device("device".into(), Some(id.into()));
            Ok(())
        })
        .unwrap();
}

fn retry(id: &str) -> UpgradeRetry {
    UpgradeRetry {
        item_id: id.into(),
        replacement_path: None,
        archive_entry: None,
        save_unit_id: None,
    }
}

#[test]
fn upgrade_starts_explicitly_resumes_and_keeps_pending_history_and_originals() {
    let (root, game) = fixture();
    add_snapshot(root.path(), &game, "healthy", None, true);
    add_snapshot(root.path(), &game, "missing", Some("healthy"), false);
    add_snapshot(root.path(), &game, "damaged", Some("missing"), false);
    fs::write(root.path().join("traveller/damaged.zip"), b"damaged").unwrap();
    let job = engine(root.path(), &game);
    let preview = job.inspect().unwrap();
    assert!(!preview.started);
    assert_eq!(preview.remaining, 3);
    assert!(preview.estimated_extra_bytes > 0);
    assert!(job.step().is_err());
    assert!(!root.path().join(".rgsm-upgrade").exists());
    job.start().unwrap();
    let first = job.step().unwrap();
    assert_eq!(first.completed, 1);
    assert_eq!(first.original_count, 1);
    assert!(!root.path().join("traveller/healthy.zip").exists());
    let journal = job.load().unwrap();
    assert!(Path::new(&journal.items[0].retained_path).is_file());

    let resumed = engine(root.path(), &game);
    assert_eq!(resumed.inspect().unwrap().remaining, 2);
    resumed.step().unwrap();
    let result = resumed.step().unwrap();
    assert_eq!(result.completed, 1);
    assert_eq!(result.pending.len(), 2);
    assert!(
        result
            .pending
            .iter()
            .any(|pending| pending.issue.kind == UpgradeIssueKind::MissingArchive)
    );
    let catalog = SnapshotCatalog::new(root.path(), &game, &"device".into())
        .read()
        .unwrap();
    assert_eq!(catalog.backups.len(), 3);
    assert_eq!(catalog.backups[1].parent.as_deref(), Some("healthy"));
    assert_eq!(
        catalog.device_heads.get("device").map(String::as_str),
        Some("damaged")
    );
    assert_eq!(catalog.backups[0].describe, "unchanged note");
    assert!(catalog.backups[0].created_at.is_none());

    write_zip(&root.path().join("traveller/missing.zip"));
    let missing = result
        .pending
        .iter()
        .find(|item| item.snapshot_id == "missing")
        .unwrap();
    resumed.retry(retry(&missing.id)).unwrap();
    assert_eq!(resumed.step().unwrap().completed, 2);
    let cleaned = resumed.cleanup_originals().unwrap();
    assert_eq!(cleaned.original_count, 0);
    assert_eq!(cleaned.pending.len(), 1);
    assert_eq!(
        fs::read(root.path().join("traveller/damaged.zip")).unwrap(),
        b"damaged"
    );
    assert!(
        catalog.backups[0]
            .archive_name
            .as_ref()
            .unwrap()
            .starts_with("unknown-time_")
    );
}

#[test]
fn interruptions_before_and_after_catalog_commit_resume_the_same_output() {
    for point in ["written", "prepared", "committed"] {
        let (root, game) = fixture();
        add_snapshot(root.path(), &game, "snapshot", None, true);
        let job = engine(root.path(), &game);
        job.start().unwrap();
        let mut journal = job.load().unwrap();
        let mut prepared = journal.items[0].clone();
        job.prepare(&mut prepared).unwrap();
        let output = job.destination(&prepared).unwrap();
        let bytes = fs::read(&output).unwrap();
        if point != "written" {
            journal.items[0] = prepared.clone();
            job.store(&journal).unwrap();
        }
        if point == "committed" {
            job.commit(&prepared).unwrap();
        }
        let resumed = engine(root.path(), &game);
        assert_eq!(resumed.step().unwrap().completed, 1, "{point}");
        assert_eq!(fs::read(&output).unwrap(), bytes, "{point}");
        let catalog = SnapshotCatalog::new(root.path(), &game, &"device".into())
            .read()
            .unwrap();
        assert_eq!(catalog.backups.len(), 1);
        assert_eq!(catalog.backups[0].archive_name, prepared.output_name);
        assert_eq!(
            catalog.device_heads.get("device").map(String::as_str),
            Some("snapshot")
        );
    }
}

#[test]
fn cleanup_keeps_originals_when_the_converted_copy_was_changed() {
    let (root, game) = fixture();
    add_snapshot(root.path(), &game, "snapshot", None, true);
    let job = engine(root.path(), &game);
    job.start().unwrap();
    job.step().unwrap();
    let journal = job.load().unwrap();
    fs::write(
        job.destination(&journal.items[0]).unwrap(),
        b"changed after conversion",
    )
    .unwrap();
    let result = job.cleanup_originals().unwrap();
    assert_eq!(result.original_count, 1);
    assert_eq!(
        result.pending[0].issue.kind,
        UpgradeIssueKind::OriginalChanged
    );
    assert!(Path::new(&journal.items[0].retained_path).is_file());
}

#[test]
fn replacement_copy_is_only_read_and_extra_backups_upgrade_without_a_catalog() {
    let (root, game) = fixture();
    add_snapshot(root.path(), &game, "missing", None, false);
    let job = engine(root.path(), &game);
    job.start().unwrap();
    let pending = job.step().unwrap().pending.remove(0);
    let replacement = root.path().join("replacement.zip");
    write_zip(&replacement);
    let bytes = fs::read(&replacement).unwrap();
    job.retry(UpgradeRetry {
        replacement_path: Some(replacement.to_string_lossy().into_owned()),
        ..retry(&pending.id)
    })
    .unwrap();
    job.step().unwrap();
    job.cleanup_originals().unwrap();
    assert_eq!(fs::read(&replacement).unwrap(), bytes);

    let (root, game) = fixture();
    let extra = root.path().join("traveller/extra_backup");
    fs::create_dir_all(&extra).unwrap();
    write_zip(&extra.join("Overwrite_2020-01-02_03-04-05.zip"));
    let job = engine(root.path(), &game);
    assert_eq!(job.inspect().unwrap().total, 1);
    job.start().unwrap();
    assert_eq!(job.step().unwrap().completed, 1);
    let names: Vec<_> = fs::read_dir(&extra)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 1);
    assert!(names[0].starts_with("Overwrite_2020-01-02_03-04-05_"));
    assert!(names[0].ends_with(".7z"));
    assert!(!root.path().join("traveller/Backups.json").exists());
}

#[test]
fn corrupt_catalogs_are_reported_without_blocking_other_games() {
    let (root, game) = fixture();
    add_snapshot(root.path(), &game, "snapshot", None, true);
    let mut broken = game.clone();
    broken.storage_key = "broken".into();
    fs::create_dir(root.path().join("broken")).unwrap();
    fs::write(root.path().join("broken/Backups.json"), b"broken").unwrap();
    let job = LocalUpgrade::new(
        root.path().into(),
        "device".into(),
        vec![game, broken],
        CompressionPreset::Standard,
    );
    let view = job.start().unwrap();
    assert_eq!(
        view.pending[0].issue.kind,
        UpgradeIssueKind::CatalogUnreadable
    );
    assert_eq!(job.step().unwrap().completed, 1);
    assert_eq!(
        fs::read(root.path().join("broken/Backups.json")).unwrap(),
        b"broken"
    );
}
