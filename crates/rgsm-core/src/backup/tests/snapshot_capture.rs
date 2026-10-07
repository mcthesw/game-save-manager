use std::{collections::HashMap, fs, path::Path};

use crate::{
    backup::{
        ArchiveFormat, CaptureGroup, CapturePlan, CaptureSnapshotOptions, CaptureSourceKind,
        CompressionPreset, CreatedBy, Game, GameSnapshots, SaveUnit, SaveUnitType, Snapshot,
        catalog::SnapshotCatalog,
    },
    config::Config,
    device::get_current_device_id,
};

use super::utils::{ConfigFileGuard, lock_config_file};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn seed_catalog(root: &Path, game: &Game, device: &str, head: &str) -> TestResult {
    let catalog = SnapshotCatalog::new(root, game, &device.to_string());
    catalog.update::<crate::preclude::BackupError>(|snapshots| {
        snapshots.backups.push(Snapshot {
            date: head.into(),
            describe: String::new(),
            path: String::new(),
            archive_name: None,
            archive_format: ArchiveFormat::SevenZ,
            size: 0,
            parent: None,
            archive_hash: None,
            created_at: None,
            device_id: Some(device.into()),
            created_by: CreatedBy::Manual,
        });
        snapshots.set_head_for_device(device.into(), Some(head.into()));
        Ok(())
    })?;
    Ok(())
}

fn capture_with_explicit_context(distinct_root: bool) -> TestResult {
    let _lock = lock_config_file();
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    runtime.block_on(async {
        let temp = temp_dir::TempDir::new()?;
        let global_root = temp.path().join("global-backups");
        let target_root = if distinct_root {
            temp.path().join("explicit-backups")
        } else {
            global_root.clone()
        };
        let device = format!("{}-explicit", get_current_device_id());
        let source = temp.path().join("slot.sav");
        fs::write(&source, "saved progress")?;
        let game: Game = serde_json::from_value(serde_json::json!({
            "name": "Capture context", "storage_key": "capture-context",
            "save_paths": [SaveUnit::concrete(0, SaveUnitType::File, HashMap::from([
                (device.clone(), source.to_string_lossy().into_owned())
            ]), false, true)]
        }))?;
        let config = Config {
            backup_path: global_root.to_string_lossy().into_owned(),
            ..Config::default()
        };
        let _guard = ConfigFileGuard::write_config(&config)?;
        seed_catalog(
            &global_root,
            &game,
            get_current_device_id(),
            "global-parent",
        )?;
        seed_catalog(&target_root, &game, &device, "explicit-parent")?;
        let global_catalog = SnapshotCatalog::new(&global_root, &game, get_current_device_id());
        let global_before = fs::read(global_catalog.path())?;
        let target_catalog = SnapshotCatalog::new(&target_root, &game, &device);

        let plan = CapturePlan {
            groups: vec![CaptureGroup {
                relative_expression: None,
                id: 0,
                save_unit_id: 0,
                candidate_id: "concrete".into(),
                dimensions: Default::default(),
                logical_anchor: temp.path().to_path_buf(),
                source_path: source.to_string_lossy().into_owned(),
                relative_path: "slot.sav".into(),
                archive_path: "slot.sav".into(),
                kind: CaptureSourceKind::File,
                delete_before_apply: false,
            }],
        };
        let mut parent = "explicit-parent".to_string();
        for _ in 0..2 {
            let created = game
                .create_snapshot_from_capture_plan(
                    &plan,
                    CaptureSnapshotOptions {
                        backup_base: &target_root,
                        device_id: &device,
                        preset: CompressionPreset::Store,
                        describe: "Explicit context",
                        parent_date: None,
                        created_by: CreatedBy::Manual,
                        source_fingerprint: None,
                        notifier: None,
                    },
                )
                .await?;
            let snapshot = created.snapshots.backups.last().unwrap();
            assert_eq!(snapshot.parent.as_deref(), Some(parent.as_str()));
            assert_eq!(snapshot.device_id.as_ref(), Some(&device));
            assert!(created.local_archive_path.starts_with(&target_root));
            assert!(created.local_archive_path.is_file());
            let persisted: GameSnapshots = target_catalog.read()?;
            assert_eq!(persisted, created.snapshots);
            assert_eq!(persisted.head_for_device(&device), Some(&snapshot.date));
            assert_eq!(
                global_catalog
                    .read()?
                    .head_for_device(get_current_device_id())
                    .map(String::as_str),
                Some("global-parent")
            );
            if distinct_root {
                assert_eq!(fs::read(global_catalog.path())?, global_before);
            }
            parent.clone_from(&snapshot.date);
        }
        Ok(())
    })
}

#[test]
fn capture_uses_explicit_backup_root_for_archive_and_catalog() -> TestResult {
    capture_with_explicit_context(true)
}

#[test]
fn capture_uses_explicit_device_for_parent_and_head() -> TestResult {
    capture_with_explicit_context(false)
}
