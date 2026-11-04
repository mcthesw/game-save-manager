use std::collections::HashMap;

use super::*;
use crate::backup::SaveUnitSource;
use crate::path_pattern::{ManifestPathConstraints, ManifestPathPattern};
use crate::path_resolution::CandidateDimensions;

mod auto_cleanup {
    use std::{fs, sync::Arc};

    use crate::backup::{ArchiveFormat, Snapshot};
    use crate::config::{
        Config, ConfigTestStateGuard, activate_cloud_namespace_v2, cloud_bootstrap_inputs,
        set_config_local,
    };
    use crate::hooks::HookPipeline;

    use super::*;

    /// Three automatic snapshots on a game whose limit is one.
    fn fixture(root: &std::path::Path) -> (ConfigTestStateGuard, Game) {
        let game: Game = serde_json::from_value(serde_json::json!({
            "name": "Auto game", "storage_key": "auto-game", "save_paths": []
        }))
        .unwrap();
        let config = Config {
            backup_path: root.join("archives").to_string_lossy().into_owned(),
            games: vec![game.clone()],
            ..Config::default()
        };
        let guard = ConfigTestStateGuard::replace_with(&config).unwrap();
        set_config_local(&config).unwrap();
        let directory = root.join("archives/auto-game");
        fs::create_dir_all(&directory).unwrap();
        let mut snapshots = GameSnapshots::new("Auto game");
        for (index, date) in ["a", "b", "c"].into_iter().enumerate() {
            let path = directory.join(format!("{date}.7z"));
            fs::write(&path, date).unwrap();
            snapshots.backups.push(Snapshot {
                date: date.into(),
                describe: String::new(),
                path: path.to_string_lossy().into_owned(),
                archive_name: None,
                archive_format: ArchiveFormat::SevenZ,
                size: 1,
                parent: (index > 0).then(|| ["a", "b"][index - 1].to_string()),
                archive_hash: None,
                created_at: Some(index as i64),
                device_id: None,
                created_by: CreatedBy::Timer,
            });
        }
        game.set_game_snapshots_info(&snapshots).unwrap();
        (guard, game)
    }

    fn run(game: &Game) -> Result<(), BackupError> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(
                ServiceContext::new(Arc::new(HookPipeline::new(vec![])))
                    .cleanup_old_auto_backups(game, 1),
            )
    }

    #[test]
    fn local_game_drops_oldest_automatic_snapshots() {
        let _lock = crate::config::lock_config_test_file();
        let root = temp_dir::TempDir::new().unwrap();
        let (_guard, game) = fixture(root.path());

        run(&game).unwrap();

        let dates: Vec<_> = game
            .get_game_snapshots_info()
            .unwrap()
            .backups
            .into_iter()
            .map(|snapshot| snapshot.date)
            .collect();
        assert_eq!(dates, ["c"]);
    }

    #[test]
    fn shared_game_keeps_every_snapshot_for_shared_retention() {
        let _lock = crate::config::lock_config_test_file();
        let root = temp_dir::TempDir::new().unwrap();
        let (_guard, game) = fixture(root.path());
        let (library, profile, _) = cloud_bootstrap_inputs().unwrap();
        activate_cloud_namespace_v2(&library, &profile, "test-library").unwrap();

        run(&game).unwrap();

        assert_eq!(game.get_game_snapshots_info().unwrap().backups.len(), 3);
        assert!(root.path().join("archives/auto-game/a.7z").exists());
    }
}

#[test]
fn automatic_backup_skips_a_game_that_is_being_restored() {
    let _lock = crate::config::lock_config_test_file();
    let root = temp_dir::TempDir::new().unwrap();
    let save = root.path().join("save.sav");
    std::fs::write(&save, "state").unwrap();
    let paths = HashMap::from([(
        crate::device::get_current_device_id().clone(),
        save.to_string_lossy().into_owned(),
    )]);
    let game: Game = serde_json::from_value(serde_json::json!({
        "name": "Busy game", "storage_key": "busy-game",
        "save_paths": [SaveUnit::concrete(1, SaveUnitType::File, paths, false, true)],
        "next_save_unit_id": 2,
    }))
    .unwrap();
    let config = crate::config::Config {
        backup_path: root.path().join("archives").to_string_lossy().into_owned(),
        games: vec![game.clone()],
        ..Default::default()
    };
    let _guard = crate::config::ConfigTestStateGuard::replace_with(&config).unwrap();
    crate::config::set_config_local(&config).unwrap();
    game.set_game_snapshots_info(&GameSnapshots::new("Busy game"))
        .unwrap();
    let service = ServiceContext::new(std::sync::Arc::new(crate::hooks::HookPipeline::new(vec![])));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let backup = || {
        runtime.block_on(service.create_snapshot_if_changed(
            &game,
            "auto",
            CreatedBy::Timer,
            HookSource::TimerAutoBackup,
            None,
        ))
    };

    let restoring = runtime.block_on(lock_game(&game));
    assert_eq!(backup().unwrap(), TimerSnapshotDecision::SkippedBusy);
    drop(restoring);
    assert_eq!(backup().unwrap(), TimerSnapshotDecision::Created);
}

fn legacy_v2_group(kind: CaptureSourceKind) -> ArchiveCaptureGroup {
    ArchiveCaptureGroup {
        relative_expression: None,
        id: 0,
        save_unit_id: 12,
        candidate_id: "legacy-v2".to_string(),
        dimensions: CandidateDimensions::default(),
        relative_path: String::new(),
        archive_path: "12/registry.reg".to_string(),
        kind,
        delete_before_apply: false,
        source_path_diagnostic: None,
    }
}

#[test]
fn batch_error_state_retains_the_first_failure() {
    let mut first_error = None;
    retain_first_error(&mut first_error, Err(BackupError::NoDataMatched));
    retain_first_error(&mut first_error, Ok(()));
    retain_first_error(&mut first_error, Err(BackupError::NoBackupAvailable));

    assert!(matches!(first_error, Some(BackupError::NoDataMatched)));
}

#[test]
fn legacy_v2_concrete_type_overrides_archive_filename_inference() {
    let mut groups = vec![legacy_v2_group(CaptureSourceKind::Registry)];
    let units = vec![SaveUnit::concrete(
        12,
        SaveUnitType::File,
        HashMap::new(),
        true,
        true,
    )];

    apply_legacy_v2_save_unit_metadata(&mut groups, &units);

    assert_eq!(groups[0].kind, CaptureSourceKind::File);
    assert!(groups[0].delete_before_apply);
}

#[test]
fn legacy_v2_dynamic_pattern_keeps_archive_shape_inference() {
    let mut groups = vec![legacy_v2_group(CaptureSourceKind::Directory)];
    let units = vec![SaveUnit {
        id: 12,
        source: SaveUnitSource::ManifestPattern {
            expected_type: None,
            pattern: ManifestPathPattern::new("<home>/Saves/*"),
            constraints: ManifestPathConstraints::default(),
        },
        delete_before_apply: true,
        enabled: true,
    }];

    apply_legacy_v2_save_unit_metadata(&mut groups, &units);

    assert_eq!(groups[0].kind, CaptureSourceKind::Directory);
    assert!(groups[0].delete_before_apply);
}

#[test]
fn legacy_v2_typed_pattern_preserves_declared_folder_kind() {
    let mut groups = vec![legacy_v2_group(CaptureSourceKind::File)];
    let units = vec![SaveUnit {
        id: 12,
        source: SaveUnitSource::ManifestPattern {
            expected_type: Some(SaveUnitType::Folder),
            pattern: ManifestPathPattern::new("<root>/Saved"),
            constraints: ManifestPathConstraints::default(),
        },
        delete_before_apply: true,
        enabled: true,
    }];

    apply_legacy_v2_save_unit_metadata(&mut groups, &units);

    assert_eq!(groups[0].kind, CaptureSourceKind::Directory);
    assert!(groups[0].delete_before_apply);
}
