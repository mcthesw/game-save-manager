use super::utils::{ConfigFileGuard, lock_config_file};
use crate::backup::archive::system_time_to_zip_datetime;
use crate::backup::state_fingerprint::{fingerprint_source_state, fingerprint_zip_state};
use crate::backup::{
    CreatedBy, Game, GameDraft, GameSnapshots, SaveUnit, SaveUnitDraft, SaveUnitType, Snapshot,
    TIMER_AUTO_BACKUP_DESCRIPTION, TimerSnapshotDecision,
};
use crate::config::{Config, get_config};
use crate::device::get_current_device_id;
use filetime::{FileTime, set_file_mtime};
use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File};
use std::future::Future;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, SystemTime};
use zip::{ZipWriter, write::SimpleFileOptions};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn run_async_test<F>(future: F) -> TestResult
where
    F: Future<Output = TestResult>,
{
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(future)
}

fn build_file_save_unit(path: &Path) -> SaveUnit {
    let mut paths = HashMap::new();
    paths.insert(
        get_current_device_id().clone(),
        path.to_string_lossy().to_string(),
    );
    SaveUnit::concrete(0, SaveUnitType::File, paths, false, true)
}

fn init_backups_json(game_name: &str, backup_root: &Path) -> TestResult {
    let game_dir = backup_root.join(game_name);
    fs::create_dir_all(&game_dir)?;

    let snapshots = GameSnapshots::new(game_name);

    fs::write(
        game_dir.join("Backups.json"),
        serde_json::to_string_pretty(&snapshots)?,
    )?;
    Ok(())
}

fn overwrite_save_file(path: &Path, content: &[u8], mtime: SystemTime) -> TestResult {
    fs::write(path, content)?;
    set_file_mtime(path, FileTime::from_system_time(mtime))?;
    Ok(())
}

fn auto_backup_count(game: &Game) -> Result<usize, Box<dyn std::error::Error>> {
    let snapshots = game.get_game_snapshots_info()?;
    Ok(snapshots
        .backups
        .iter()
        .filter(|snapshot| snapshot.created_by == CreatedBy::Timer)
        .count())
}

fn create_legacy_auto_snapshot(
    game: &Game,
    save_file: &Path,
    date: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let backup_root = crate::config::get_backup_path()?.join(&game.name);
    fs::create_dir_all(&backup_root)?;
    let zip_path = backup_root.join(format!("{date}.zip"));

    let file_name = save_file
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid save file name")?;
    let file_mtime = fs::metadata(save_file)?.modified()?;
    let zip_time = system_time_to_zip_datetime(file_mtime);
    let bytes = fs::read(save_file)?;

    let mut zip_writer = ZipWriter::new(File::create(&zip_path)?);
    zip_writer.start_file(
        file_name,
        SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Bzip2)
            .last_modified_time(zip_time),
    )?;
    zip_writer.write_all(&bytes)?;
    zip_writer.finish()?;

    let mut snapshots = game.get_game_snapshots_info()?;
    snapshots.backups.push(Snapshot {
        date: date.to_string(),
        describe: TIMER_AUTO_BACKUP_DESCRIPTION.to_string(),
        path: zip_path.to_string_lossy().to_string(),
        archive_format: crate::backup::ArchiveFormat::Zip,
        size: fs::metadata(&zip_path)?.len(),
        parent: None,
        archive_hash: None,
        device_id: None,
        created_by: CreatedBy::Timer,
    });
    snapshots.set_current_device_head(Some(date.to_string()));
    game.set_game_snapshots_info(&snapshots)?;
    Ok(())
}

fn restore_config_guard(config: &Config) -> Result<ConfigFileGuard, Box<dyn std::error::Error>> {
    ConfigFileGuard::write_config(config)
}

#[test]
fn timer_backup_skips_when_unchanged() -> TestResult {
    let _config_lock = lock_config_file();
    run_async_test(async {
        let initial_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_710_000_001);

        let temp_dir = temp_dir::TempDir::new()?;
        let backup_root = temp_dir.path().join("backup");
        fs::create_dir_all(&backup_root)?;

        let config = Config {
            backup_path: backup_root.to_string_lossy().to_string(),
            ..Config::default()
        };
        let _config_guard = restore_config_guard(&config)?;

        let save_file = temp_dir.path().join("profile.sav");
        overwrite_save_file(&save_file, b"stable-content", initial_time)?;

        let game_name = "timer_skip_game";
        init_backups_json(game_name, &backup_root)?;
        let game = Game {
            name: game_name.to_string(),
            storage_key: String::new(),
            save_paths: vec![build_file_save_unit(&save_file)],
            game_paths: HashMap::new(),
            next_save_unit_id: 1,
            cloud_sync_enabled: true,
            auto_backup: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        };

        let first = game
            .create_timer_snapshot_if_changed(TIMER_AUTO_BACKUP_DESCRIPTION)
            .await?;
        assert_eq!(first, TimerSnapshotDecision::Created);

        let second = game
            .create_timer_snapshot_if_changed(TIMER_AUTO_BACKUP_DESCRIPTION)
            .await?;
        assert_eq!(second, TimerSnapshotDecision::SkippedUnchanged);
        assert_eq!(auto_backup_count(&game)?, 1);
        Ok(())
    })
}

#[test]
fn timer_backup_creates_when_changed() -> TestResult {
    let _config_lock = lock_config_file();
    run_async_test(async {
        let initial_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_710_000_100);

        let temp_dir = temp_dir::TempDir::new()?;
        let backup_root = temp_dir.path().join("backup");
        fs::create_dir_all(&backup_root)?;

        let config = Config {
            backup_path: backup_root.to_string_lossy().to_string(),
            ..Config::default()
        };
        let _config_guard = restore_config_guard(&config)?;

        let save_file = temp_dir.path().join("profile.sav");
        overwrite_save_file(&save_file, b"content-a", initial_time)?;

        let game_name = "timer_changed_game";
        init_backups_json(game_name, &backup_root)?;
        let game = Game {
            name: game_name.to_string(),
            storage_key: String::new(),
            save_paths: vec![build_file_save_unit(&save_file)],
            game_paths: HashMap::new(),
            next_save_unit_id: 1,
            cloud_sync_enabled: true,
            auto_backup: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        };

        let first = game
            .create_timer_snapshot_if_changed(TIMER_AUTO_BACKUP_DESCRIPTION)
            .await?;
        assert_eq!(first, TimerSnapshotDecision::Created);

        tokio::time::sleep(Duration::from_secs(1)).await;
        overwrite_save_file(
            &save_file,
            b"content-b",
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_710_000_200),
        )?;

        let second = game
            .create_timer_snapshot_if_changed(TIMER_AUTO_BACKUP_DESCRIPTION)
            .await?;
        assert_eq!(second, TimerSnapshotDecision::Created);
        assert_eq!(auto_backup_count(&game)?, 2);
        Ok(())
    })
}

#[test]
fn timer_backup_compares_only_latest_auto_backup() -> TestResult {
    let _config_lock = lock_config_file();
    run_async_test(async {
        let initial_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_710_001_000);
        let changed_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_710_001_100);

        let temp_dir = temp_dir::TempDir::new()?;
        let backup_root = temp_dir.path().join("backup");
        fs::create_dir_all(&backup_root)?;

        let config = Config {
            backup_path: backup_root.to_string_lossy().to_string(),
            ..Config::default()
        };
        let _config_guard = restore_config_guard(&config)?;

        let save_file = temp_dir.path().join("profile.sav");
        overwrite_save_file(&save_file, b"content-a", initial_time)?;

        let game_name = "timer_only_auto_compare";
        init_backups_json(game_name, &backup_root)?;
        let game = Game {
            name: game_name.to_string(),
            storage_key: String::new(),
            save_paths: vec![build_file_save_unit(&save_file)],
            game_paths: HashMap::new(),
            next_save_unit_id: 1,
            cloud_sync_enabled: true,
            auto_backup: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        };

        let first = game
            .create_timer_snapshot_if_changed(TIMER_AUTO_BACKUP_DESCRIPTION)
            .await?;
        assert_eq!(first, TimerSnapshotDecision::Created);

        tokio::time::sleep(Duration::from_secs(1)).await;
        overwrite_save_file(&save_file, b"content-b", changed_time)?;
        game.create_snapshot("Manual Snapshot").await?;

        tokio::time::sleep(Duration::from_secs(1)).await;
        overwrite_save_file(&save_file, b"content-a", initial_time)?;

        let decision = game
            .create_timer_snapshot_if_changed(TIMER_AUTO_BACKUP_DESCRIPTION)
            .await?;
        assert_eq!(decision, TimerSnapshotDecision::SkippedUnchanged);
        assert_eq!(auto_backup_count(&game)?, 1);
        Ok(())
    })
}

#[test]
fn legacy_auto_snapshot_creates_once_before_dedup() -> TestResult {
    let _config_lock = lock_config_file();
    run_async_test(async {
        let initial_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_710_002_000);

        let temp_dir = temp_dir::TempDir::new()?;
        let backup_root = temp_dir.path().join("backup");
        fs::create_dir_all(&backup_root)?;

        let config = Config {
            backup_path: backup_root.to_string_lossy().to_string(),
            ..Config::default()
        };
        let _config_guard = restore_config_guard(&config)?;

        let save_file = temp_dir.path().join("profile.sav");
        overwrite_save_file(&save_file, b"legacy-content", initial_time)?;

        let game_name = "legacy_timer_game";
        init_backups_json(game_name, &backup_root)?;
        let game = Game {
            name: game_name.to_string(),
            storage_key: String::new(),
            save_paths: vec![build_file_save_unit(&save_file)],
            game_paths: HashMap::new(),
            next_save_unit_id: 1,
            cloud_sync_enabled: true,
            auto_backup: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        };

        create_legacy_auto_snapshot(&game, &save_file, "2000-01-01_00-00-00")?;
        let first = game
            .create_timer_snapshot_if_changed(TIMER_AUTO_BACKUP_DESCRIPTION)
            .await?;
        assert_eq!(first, TimerSnapshotDecision::Created);

        let second = game
            .create_timer_snapshot_if_changed(TIMER_AUTO_BACKUP_DESCRIPTION)
            .await?;
        assert_eq!(second, TimerSnapshotDecision::SkippedUnchanged);
        assert_eq!(auto_backup_count(&game)?, 2);
        Ok(())
    })
}

#[test]
fn fingerprint_source_and_zip_match_for_fresh_snapshot() -> TestResult {
    let _config_lock = lock_config_file();
    run_async_test(async {
        let initial_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_710_003_000);

        let temp_dir = temp_dir::TempDir::new()?;
        let backup_root = temp_dir.path().join("backup");
        fs::create_dir_all(&backup_root)?;

        let config = Config {
            backup_path: backup_root.to_string_lossy().to_string(),
            ..Config::default()
        };
        let _config_guard = restore_config_guard(&config)?;

        let save_file = temp_dir.path().join("profile.sav");
        overwrite_save_file(&save_file, b"fresh-content", initial_time)?;

        let game_name = "fingerprint_match_game";
        init_backups_json(game_name, &backup_root)?;
        let game = Game {
            name: game_name.to_string(),
            storage_key: String::new(),
            save_paths: vec![build_file_save_unit(&save_file)],
            game_paths: HashMap::new(),
            next_save_unit_id: 1,
            cloud_sync_enabled: true,
            auto_backup: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        };

        game.create_snapshot("Manual Snapshot").await?;
        let snapshots = game.get_game_snapshots_info()?;
        let latest_snapshot = snapshots
            .backups
            .last()
            .ok_or("missing latest snapshot for fingerprint test")?;

        let source_fp = fingerprint_source_state(&game.save_paths, None)?;
        let zip_fp = fingerprint_zip_state(Path::new(&latest_snapshot.path))?
            .ok_or("zip fingerprint should exist for fresh snapshots")?;
        assert_eq!(source_fp, zip_fp);
        Ok(())
    })
}

// --- batch_delete_snapshots tests ---

/// Create a dummy zip and snapshot entry with a specific date and parent.
fn insert_snapshot(
    game: &Game,
    backup_root: &Path,
    date: &str,
    parent: Option<&str>,
) -> TestResult {
    let game_dir = backup_root.join(&game.name);
    let zip_path = game_dir.join(format!("{date}.zip"));

    // Write a minimal valid zip
    let mut zw = ZipWriter::new(File::create(&zip_path)?);
    zw.start_file("dummy.txt", SimpleFileOptions::default())?;
    zw.write_all(date.as_bytes())?;
    zw.finish()?;

    let mut snapshots = game.get_game_snapshots_info()?;
    snapshots.backups.push(Snapshot {
        date: date.to_string(),
        describe: "test".to_string(),
        path: zip_path.to_string_lossy().to_string(),
        archive_format: crate::backup::ArchiveFormat::Zip,
        size: fs::metadata(&zip_path)?.len(),
        parent: parent.map(|s| s.to_string()),
        archive_hash: None,
        device_id: None,
        created_by: Default::default(),
    });
    snapshots.set_current_device_head(Some(date.to_string()));
    game.set_game_snapshots_info(&snapshots)?;
    Ok(())
}

fn insert_v4_snapshot(
    game: &Game,
    backup_root: &Path,
    date: &str,
    parent: Option<&str>,
) -> TestResult {
    let game_dir = backup_root.join(&game.name);
    let archive_path = game_dir.join(format!("{date}.7z"));
    fs::write(&archive_path, b"archive-v4")?;
    let mut snapshots = game.get_game_snapshots_info()?;
    snapshots.backups.push(Snapshot {
        date: date.to_string(),
        describe: "test".to_string(),
        path: archive_path.to_string_lossy().to_string(),
        archive_format: crate::backup::ArchiveFormat::SevenZ,
        size: fs::metadata(&archive_path)?.len(),
        parent: parent.map(str::to_string),
        archive_hash: None,
        device_id: None,
        created_by: Default::default(),
    });
    snapshots.set_current_device_head(Some(date.to_string()));
    game.set_game_snapshots_info(&snapshots)?;
    Ok(())
}

fn make_test_game(game_name: &str, backup_root: &Path) -> Result<Game, Box<dyn std::error::Error>> {
    init_backups_json(game_name, backup_root)?;
    Ok(Game {
        name: game_name.to_string(),
        storage_key: String::new(),
        save_paths: Vec::new(),
        game_paths: HashMap::new(),
        next_save_unit_id: 0,
        cloud_sync_enabled: true,
        auto_backup: None,
        ludusavi_meta: None,
        device_bindings: std::collections::HashMap::new(),
    })
}

fn make_test_game_with_storage_key(
    game_name: &str,
    storage_key: &str,
    backup_root: &Path,
) -> Result<Game, Box<dyn std::error::Error>> {
    init_backups_json(storage_key, backup_root)?;
    Ok(Game {
        name: game_name.to_string(),
        storage_key: storage_key.to_string(),
        save_paths: Vec::new(),
        game_paths: HashMap::new(),
        next_save_unit_id: 0,
        cloud_sync_enabled: true,
        auto_backup: None,
        ludusavi_meta: None,
        device_bindings: std::collections::HashMap::new(),
    })
}

#[test]
fn batch_delete_removes_snapshots_and_returns_remote_paths() -> TestResult {
    let _config_lock = lock_config_file();
    run_async_test(async {
        let temp_dir = temp_dir::TempDir::new()?;
        let backup_root = temp_dir.path().join("backup");
        fs::create_dir_all(&backup_root)?;

        let config = Config {
            backup_path: backup_root.to_string_lossy().to_string(),
            ..Config::default()
        };
        let _config_guard = restore_config_guard(&config)?;

        let game = make_test_game("batch_del_basic", &backup_root)?;
        insert_snapshot(&game, &backup_root, "2025-01-01_00-00-00", None)?;
        insert_snapshot(
            &game,
            &backup_root,
            "2025-01-02_00-00-00",
            Some("2025-01-01_00-00-00"),
        )?;
        insert_snapshot(
            &game,
            &backup_root,
            "2025-01-03_00-00-00",
            Some("2025-01-02_00-00-00"),
        )?;

        let dates = vec![
            "2025-01-01_00-00-00".to_string(),
            "2025-01-03_00-00-00".to_string(),
        ];
        let result = game.batch_delete_snapshots(&dates).await?;

        // Only B remains
        assert_eq!(result.snapshots.backups.len(), 1);
        assert_eq!(result.snapshots.backups[0].date, "2025-01-02_00-00-00");

        // Correct remote paths
        assert_eq!(result.deleted_remote_paths.len(), 2);
        assert!(
            result
                .deleted_remote_paths
                .iter()
                .any(|p| p.contains("2025-01-01_00-00-00"))
        );
        assert!(
            result
                .deleted_remote_paths
                .iter()
                .any(|p| p.contains("2025-01-03_00-00-00"))
        );

        // Zip files removed
        assert!(
            !backup_root
                .join("batch_del_basic/2025-01-01_00-00-00.zip")
                .exists()
        );
        assert!(
            !backup_root
                .join("batch_del_basic/2025-01-03_00-00-00.zip")
                .exists()
        );
        assert!(
            backup_root
                .join("batch_del_basic/2025-01-02_00-00-00.zip")
                .exists()
        );

        Ok(())
    })
}

#[test]
fn batch_delete_uses_each_snapshot_container_in_mixed_history() -> TestResult {
    let _config_lock = lock_config_file();
    run_async_test(async {
        let temp_dir = temp_dir::TempDir::new()?;
        let backup_root = temp_dir.path().join("backup");
        fs::create_dir_all(&backup_root)?;
        let config = Config {
            backup_path: backup_root.to_string_lossy().to_string(),
            ..Config::default()
        };
        let _config_guard = restore_config_guard(&config)?;
        let game = make_test_game("mixed_delete", &backup_root)?;
        insert_snapshot(&game, &backup_root, "legacy", None)?;
        insert_v4_snapshot(&game, &backup_root, "v4", Some("legacy"))?;

        let result = game
            .batch_delete_snapshots(&["legacy".into(), "v4".into()])
            .await?;

        assert!(result.snapshots.backups.is_empty());
        assert!(!backup_root.join("mixed_delete/legacy.zip").exists());
        assert!(!backup_root.join("mixed_delete/v4.7z").exists());
        assert_eq!(
            result.deleted_remote_paths,
            vec![
                "save_data/mixed_delete/legacy.zip".to_string(),
                "save_data/mixed_delete/v4.7z".to_string(),
            ]
        );
        Ok(())
    })
}

#[test]
fn batch_delete_reparents_through_deleted_chain() -> TestResult {
    let _config_lock = lock_config_file();
    run_async_test(async {
        let temp_dir = temp_dir::TempDir::new()?;
        let backup_root = temp_dir.path().join("backup");
        fs::create_dir_all(&backup_root)?;

        let config = Config {
            backup_path: backup_root.to_string_lossy().to_string(),
            ..Config::default()
        };
        let _config_guard = restore_config_guard(&config)?;

        // Build chain: A → B → C → D
        let game = make_test_game("batch_del_chain", &backup_root)?;
        insert_snapshot(&game, &backup_root, "A", None)?;
        insert_snapshot(&game, &backup_root, "B", Some("A"))?;
        insert_snapshot(&game, &backup_root, "C", Some("B"))?;
        insert_snapshot(&game, &backup_root, "D", Some("C"))?;

        // Delete B and C (middle of chain)
        let result = game
            .batch_delete_snapshots(&["B".to_string(), "C".to_string()])
            .await?;

        assert_eq!(result.snapshots.backups.len(), 2);
        let d_snapshot = result
            .snapshots
            .backups
            .iter()
            .find(|s| s.date == "D")
            .unwrap();
        // D's parent should now be A (skipped over deleted B and C)
        assert_eq!(d_snapshot.parent.as_deref(), Some("A"));

        let a_snapshot = result
            .snapshots
            .backups
            .iter()
            .find(|s| s.date == "A")
            .unwrap();
        assert_eq!(a_snapshot.parent, None);

        Ok(())
    })
}

#[test]
fn v2_tombstone_forget_preserves_surviving_parent_links_and_clears_heads() -> TestResult {
    let _config_lock = lock_config_file();
    run_async_test(async {
        let temp_dir = temp_dir::TempDir::new()?;
        let backup_root = temp_dir.path().join("backup");
        fs::create_dir_all(&backup_root)?;
        let config = Config {
            backup_path: backup_root.to_string_lossy().to_string(),
            ..Config::default()
        };
        let _config_guard = restore_config_guard(&config)?;
        let game = make_test_game("v2_tombstone", &backup_root)?;
        insert_snapshot(&game, &backup_root, "A", None)?;
        insert_snapshot(&game, &backup_root, "B", Some("A"))?;
        insert_snapshot(&game, &backup_root, "C", Some("B"))?;
        let mut snapshots = game.get_game_snapshots_info()?;
        snapshots.set_current_device_head(Some("B".into()));
        game.set_game_snapshots_info(&snapshots)?;

        assert_eq!(
            game.forget_v2_tombstones(&BTreeSet::from(["B".to_string()]))?,
            1
        );

        let snapshots = game.get_game_snapshots_info()?;
        assert!(
            snapshots
                .backups
                .iter()
                .all(|snapshot| snapshot.date != "B")
        );
        assert_eq!(
            snapshots
                .backups
                .iter()
                .find(|snapshot| snapshot.date == "C")
                .and_then(|snapshot| snapshot.parent.as_deref()),
            Some("B")
        );
        assert!(snapshots.current_device_head().is_none());
        Ok(())
    })
}

#[test]
fn batch_delete_updates_head_to_newest_remaining() -> TestResult {
    let _config_lock = lock_config_file();
    run_async_test(async {
        let temp_dir = temp_dir::TempDir::new()?;
        let backup_root = temp_dir.path().join("backup");
        fs::create_dir_all(&backup_root)?;

        let config = Config {
            backup_path: backup_root.to_string_lossy().to_string(),
            ..Config::default()
        };
        let _config_guard = restore_config_guard(&config)?;

        // A → B → C, HEAD = C
        let game = make_test_game("batch_del_head", &backup_root)?;
        insert_snapshot(&game, &backup_root, "2025-01-01_00-00-00", None)?;
        insert_snapshot(
            &game,
            &backup_root,
            "2025-01-02_00-00-00",
            Some("2025-01-01_00-00-00"),
        )?;
        insert_snapshot(
            &game,
            &backup_root,
            "2025-01-03_00-00-00",
            Some("2025-01-02_00-00-00"),
        )?;

        // Delete HEAD (C)
        let result = game
            .batch_delete_snapshots(&["2025-01-03_00-00-00".to_string()])
            .await?;

        // HEAD should move to newest remaining (B)
        assert_eq!(
            result.snapshots.current_device_head().map(String::as_str),
            Some("2025-01-02_00-00-00")
        );

        Ok(())
    })
}

#[test]
fn batch_delete_empty_dates_is_noop() -> TestResult {
    let _config_lock = lock_config_file();
    run_async_test(async {
        let temp_dir = temp_dir::TempDir::new()?;
        let backup_root = temp_dir.path().join("backup");
        fs::create_dir_all(&backup_root)?;

        let config = Config {
            backup_path: backup_root.to_string_lossy().to_string(),
            ..Config::default()
        };
        let _config_guard = restore_config_guard(&config)?;

        let game = make_test_game("batch_del_empty", &backup_root)?;
        insert_snapshot(&game, &backup_root, "2025-01-01_00-00-00", None)?;

        let result = game.batch_delete_snapshots(&[]).await?;

        assert_eq!(result.snapshots.backups.len(), 1);
        assert!(result.deleted_remote_paths.is_empty());

        Ok(())
    })
}

#[test]
fn delete_game_clears_quick_action_reference_by_storage_key() -> TestResult {
    let _config_lock = lock_config_file();
    run_async_test(async {
        let temp_dir = temp_dir::TempDir::new()?;
        let backup_root = temp_dir.path().join("backup");
        fs::create_dir_all(&backup_root)?;

        let deleted_game =
            make_test_game_with_storage_key("Deleted Game", "deleted-game-key", &backup_root)?;
        let remaining_game =
            make_test_game_with_storage_key("Remaining Game", "remaining-game-key", &backup_root)?;
        let mut config = Config {
            backup_path: backup_root.to_string_lossy().to_string(),
            games: vec![deleted_game.clone(), remaining_game.clone()],
            ..Config::default()
        };
        config.quick_action.quick_action_game_id = Some(deleted_game.storage_key.clone());
        let _config_guard = restore_config_guard(&config)?;

        deleted_game.delete_game().await?;

        let persisted = get_config()?;
        assert_eq!(persisted.games.len(), 1);
        assert_eq!(persisted.games[0].storage_key, remaining_game.storage_key);
        assert!(persisted.quick_action.quick_action_game_id.is_none());
        Ok(())
    })
}

#[test]
fn delete_game_clears_legacy_quick_action_reference_by_name() -> TestResult {
    let _config_lock = lock_config_file();
    run_async_test(async {
        let temp_dir = temp_dir::TempDir::new()?;
        let backup_root = temp_dir.path().join("backup");
        fs::create_dir_all(&backup_root)?;

        let deleted_game = make_test_game_with_storage_key(
            "Legacy Deleted Game",
            "legacy-deleted-game",
            &backup_root,
        )?;
        let remaining_game = make_test_game_with_storage_key(
            "Legacy Remaining Game",
            "legacy-remaining-game",
            &backup_root,
        )?;
        let mut config = Config {
            backup_path: backup_root.to_string_lossy().to_string(),
            games: vec![deleted_game.clone(), remaining_game.clone()],
            ..Config::default()
        };
        config.quick_action.quick_action_game_id = Some(deleted_game.name.clone());
        let _config_guard = restore_config_guard(&config)?;

        deleted_game.delete_game().await?;

        let persisted = get_config()?;
        assert_eq!(persisted.games.len(), 1);
        assert_eq!(persisted.games[0].name, remaining_game.name);
        assert!(persisted.quick_action.quick_action_game_id.is_none());
        Ok(())
    })
}

#[test]
fn normalize_save_unit_ids_reassigns_duplicates() {
    let mut game = Game {
        name: "normalize-dup".to_string(),
        storage_key: String::new(),
        save_paths: vec![
            SaveUnit {
                id: 0,
                source: crate::backup::SaveUnitSource::Concrete {
                    unit_type: SaveUnitType::File,
                    paths: HashMap::new(),
                },
                delete_before_apply: false,
                enabled: true,
            },
            SaveUnit {
                id: 0,
                source: crate::backup::SaveUnitSource::Concrete {
                    unit_type: SaveUnitType::Folder,
                    paths: HashMap::new(),
                },
                delete_before_apply: false,
                enabled: true,
            },
            SaveUnit {
                id: 2,
                source: crate::backup::SaveUnitSource::Concrete {
                    unit_type: SaveUnitType::File,
                    paths: HashMap::new(),
                },
                delete_before_apply: false,
                enabled: true,
            },
            SaveUnit {
                id: 2,
                source: crate::backup::SaveUnitSource::Concrete {
                    unit_type: SaveUnitType::Folder,
                    paths: HashMap::new(),
                },
                delete_before_apply: false,
                enabled: true,
            },
        ],
        game_paths: HashMap::new(),
        next_save_unit_id: 1,
        cloud_sync_enabled: true,
        auto_backup: None,
        ludusavi_meta: None,
        device_bindings: std::collections::HashMap::new(),
    };

    game.normalize_save_unit_ids();

    let ids: Vec<u32> = game.save_paths.iter().map(|u| u.id).collect();
    assert_eq!(ids, vec![0, 3, 2, 4]);
    assert_eq!(game.next_save_unit_id, 5);
}

#[test]
fn normalize_save_unit_ids_assigns_sequential_from_legacy_defaults() {
    let mut game = Game {
        name: "normalize-legacy".to_string(),
        storage_key: String::new(),
        save_paths: vec![
            SaveUnit {
                id: 0,
                source: crate::backup::SaveUnitSource::Concrete {
                    unit_type: SaveUnitType::File,
                    paths: HashMap::new(),
                },
                delete_before_apply: false,
                enabled: true,
            },
            SaveUnit {
                id: 0,
                source: crate::backup::SaveUnitSource::Concrete {
                    unit_type: SaveUnitType::Folder,
                    paths: HashMap::new(),
                },
                delete_before_apply: false,
                enabled: true,
            },
            SaveUnit {
                id: 0,
                source: crate::backup::SaveUnitSource::Concrete {
                    unit_type: SaveUnitType::File,
                    paths: HashMap::new(),
                },
                delete_before_apply: false,
                enabled: true,
            },
        ],
        game_paths: HashMap::new(),
        next_save_unit_id: 0,
        cloud_sync_enabled: true,
        auto_backup: None,
        ludusavi_meta: None,
        device_bindings: std::collections::HashMap::new(),
    };

    game.normalize_save_unit_ids();

    let ids: Vec<u32> = game.save_paths.iter().map(|u| u.id).collect();
    assert_eq!(ids, vec![0, 1, 2]);
    assert_eq!(game.next_save_unit_id, 3);
}

#[test]
fn game_draft_into_game_preserves_existing_cloud_sync_enabled() {
    let existing = Game {
        name: "DraftReuse".to_string(),
        storage_key: String::new(),
        save_paths: vec![],
        game_paths: HashMap::new(),
        next_save_unit_id: 1,
        cloud_sync_enabled: false,
        auto_backup: None,
        ludusavi_meta: None,
        device_bindings: std::collections::HashMap::new(),
    };

    let draft = GameDraft {
        name: "DraftReuse".to_string(),
        save_paths: vec![],
        game_paths: HashMap::new(),
        ludusavi_meta: None,
        device_bindings: std::collections::HashMap::new(),
    };

    let game = draft.into_game(Some(&existing));

    assert!(!game.cloud_sync_enabled);
}

#[test]
fn game_draft_into_game_reuses_existing_ids_and_allocates_new_ones() {
    let device_id = "device-a".to_string();

    let mut existing_path_a = HashMap::new();
    existing_path_a.insert(device_id.clone(), "C:\\A\\save.dat".to_string());
    let mut existing_path_b = HashMap::new();
    existing_path_b.insert(device_id.clone(), "C:\\B\\save.dat".to_string());

    let existing = Game {
        name: "DraftReuse".to_string(),
        storage_key: String::new(),
        save_paths: vec![
            SaveUnit {
                id: 5,
                source: crate::backup::SaveUnitSource::Concrete {
                    unit_type: SaveUnitType::File,
                    paths: existing_path_a.clone(),
                },
                delete_before_apply: false,
                enabled: true,
            },
            SaveUnit {
                id: 7,
                source: crate::backup::SaveUnitSource::Concrete {
                    unit_type: SaveUnitType::File,
                    paths: existing_path_b,
                },
                delete_before_apply: false,
                enabled: true,
            },
        ],
        game_paths: HashMap::new(),
        next_save_unit_id: 8,
        cloud_sync_enabled: true,
        auto_backup: None,
        ludusavi_meta: None,
        device_bindings: std::collections::HashMap::new(),
    };

    let mut new_path = HashMap::new();
    new_path.insert(device_id, "C:\\C\\save.dat".to_string());
    let draft = GameDraft {
        name: "DraftReuse".to_string(),
        save_paths: vec![
            SaveUnitDraft {
                id: None,
                source: crate::backup::SaveUnitSource::Concrete {
                    unit_type: SaveUnitType::File,
                    paths: existing_path_a,
                },
                delete_before_apply: false,
                enabled: true,
            },
            SaveUnitDraft {
                id: None,
                source: crate::backup::SaveUnitSource::Concrete {
                    unit_type: SaveUnitType::File,
                    paths: new_path,
                },
                delete_before_apply: false,
                enabled: true,
            },
        ],
        game_paths: HashMap::new(),
        ludusavi_meta: None,
        device_bindings: std::collections::HashMap::new(),
    };

    let game = draft.into_game(Some(&existing));
    let ids: Vec<u32> = game.save_paths.iter().map(|u| u.id).collect();

    assert_eq!(ids, vec![5, 8]);
    assert_eq!(game.next_save_unit_id, 9);
}

#[test]
fn game_draft_into_game_preserves_explicit_id_when_path_changes() {
    let device_id = "device-a".to_string();

    let mut existing_path = HashMap::new();
    existing_path.insert(device_id.clone(), "C:\\A\\save.dat".to_string());
    let existing = Game {
        name: "DraftEdit".to_string(),
        storage_key: String::new(),
        save_paths: vec![SaveUnit {
            id: 5,
            source: crate::backup::SaveUnitSource::Concrete {
                unit_type: SaveUnitType::File,
                paths: existing_path,
            },
            delete_before_apply: false,
            enabled: true,
        }],
        game_paths: HashMap::new(),
        next_save_unit_id: 6,
        cloud_sync_enabled: true,
        auto_backup: None,
        ludusavi_meta: None,
        device_bindings: std::collections::HashMap::new(),
    };

    let mut updated_path = HashMap::new();
    updated_path.insert(device_id, "D:\\B\\renamed.dat".to_string());
    let draft = GameDraft {
        name: "DraftEdit".to_string(),
        save_paths: vec![SaveUnitDraft {
            id: Some(5),
            source: crate::backup::SaveUnitSource::Concrete {
                unit_type: SaveUnitType::File,
                paths: updated_path.clone(),
            },
            delete_before_apply: true,
            enabled: true,
        }],
        game_paths: HashMap::new(),
        ludusavi_meta: None,
        device_bindings: std::collections::HashMap::new(),
    };

    let game = draft.into_game(Some(&existing));

    assert_eq!(game.save_paths.len(), 1);
    assert_eq!(game.save_paths[0].id, 5);
    assert_eq!(game.save_paths[0].paths(), Some(&updated_path));
    assert!(game.save_paths[0].delete_before_apply);
    assert_eq!(game.next_save_unit_id, 6);
}

#[test]
fn game_draft_into_game_allocates_new_id_after_existing_row_path_edit() {
    let device_id = "device-a".to_string();

    let mut existing_path = HashMap::new();
    existing_path.insert(device_id.clone(), "C:\\A\\save.dat".to_string());
    let existing = Game {
        name: "DraftEditAndAdd".to_string(),
        storage_key: String::new(),
        save_paths: vec![SaveUnit {
            id: 5,
            source: crate::backup::SaveUnitSource::Concrete {
                unit_type: SaveUnitType::File,
                paths: existing_path.clone(),
            },
            delete_before_apply: false,
            enabled: true,
        }],
        game_paths: HashMap::new(),
        next_save_unit_id: 8,
        cloud_sync_enabled: true,
        auto_backup: None,
        ludusavi_meta: None,
        device_bindings: std::collections::HashMap::new(),
    };

    let mut updated_path = HashMap::new();
    updated_path.insert(device_id.clone(), "D:\\B\\edited.dat".to_string());
    let mut new_path = HashMap::new();
    new_path.insert(device_id, "C:\\A\\save.dat".to_string());

    let draft = GameDraft {
        name: "DraftEditAndAdd".to_string(),
        save_paths: vec![
            SaveUnitDraft {
                id: Some(5),
                source: crate::backup::SaveUnitSource::Concrete {
                    unit_type: SaveUnitType::File,
                    paths: updated_path,
                },
                delete_before_apply: false,
                enabled: true,
            },
            SaveUnitDraft {
                id: None,
                source: crate::backup::SaveUnitSource::Concrete {
                    unit_type: SaveUnitType::File,
                    paths: new_path,
                },
                delete_before_apply: false,
                enabled: true,
            },
        ],
        game_paths: HashMap::new(),
        ludusavi_meta: None,
        device_bindings: std::collections::HashMap::new(),
    };

    let game = draft.into_game(Some(&existing));
    let ids: Vec<u32> = game.save_paths.iter().map(|u| u.id).collect();

    assert_eq!(ids, vec![5, 8]);
    assert_eq!(game.next_save_unit_id, 9);
}

#[test]
fn unused_snapshot_date_skips_occupied_second() -> TestResult {
    let backup_path = Path::new("occupied-second");
    let mut infos = GameSnapshots::new("game");
    let occupied = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    infos.backups.push(Snapshot {
        date: occupied.clone(),
        describe: String::new(),
        path: String::new(),
        archive_format: crate::backup::ArchiveFormat::Zip,
        size: 0,
        parent: None,
        archive_hash: None,
        device_id: None,
        created_by: CreatedBy::Manual,
    });
    let allocated = crate::backup::game::unused_snapshot_date(backup_path, &infos)?;
    assert_ne!(allocated, occupied);
    Ok(())
}

#[test]
fn game_with_colon_in_name_can_create_snapshot() -> TestResult {
    let _config_lock = lock_config_file();
    run_async_test(async {
        let temp_dir = temp_dir::TempDir::new()?;
        let backup_root = temp_dir.path().join("backup");
        fs::create_dir_all(&backup_root)?;

        let config = Config {
            backup_path: backup_root.to_string_lossy().to_string(),
            ..Config::default()
        };
        let _config_guard = restore_config_guard(&config)?;

        let save_file = temp_dir.path().join("profile.sav");
        fs::write(&save_file, b"test-content")?;

        let game_name = "Game: The Sequel";
        let storage_key = crate::backup::storage_key::generate_storage_key(game_name);
        init_backups_json(&storage_key, &backup_root)?;

        let game = Game {
            name: game_name.to_string(),
            storage_key,
            save_paths: vec![build_file_save_unit(&save_file)],
            game_paths: HashMap::new(),
            next_save_unit_id: 1,
            cloud_sync_enabled: true,
            auto_backup: None,
            ludusavi_meta: None,
            device_bindings: std::collections::HashMap::new(),
        };

        let result = game.create_snapshot("test backup").await;
        assert!(
            result.is_ok(),
            "Should create snapshot for game with colon in name"
        );

        let snapshots = game.get_game_snapshots_info()?;
        assert_eq!(snapshots.backups.len(), 1);
        Ok(())
    })
}

#[test]
fn backup_dir_name_uses_storage_key_over_raw_name() {
    let game = Game {
        name: "Game: Special <Edition>".to_string(),
        storage_key: "Game_ Special _Edition_".to_string(),
        save_paths: vec![],
        game_paths: HashMap::new(),
        next_save_unit_id: 0,
        cloud_sync_enabled: true,
        auto_backup: None,
        ludusavi_meta: None,
        device_bindings: std::collections::HashMap::new(),
    };
    assert_eq!(game.backup_dir_name().as_ref(), "Game_ Special _Edition_");
}

#[test]
fn renamed_game_backup_folder_uses_stable_storage_key() {
    let game = Game {
        name: "Need for Speed Unbound 极品飞车22：不羁1".to_string(),
        storage_key: "Need for Speed Unbound".to_string(),
        save_paths: Vec::new(),
        game_paths: HashMap::new(),
        next_save_unit_id: 0,
        cloud_sync_enabled: true,
        auto_backup: None,
        ludusavi_meta: None,
        device_bindings: HashMap::new(),
    };

    assert_eq!(
        game.backup_folder_path(Path::new("save_data")),
        Path::new("save_data").join("Need for Speed Unbound")
    );
}

#[test]
fn backup_dir_name_fallback_sanitizes_when_storage_key_empty() {
    let game = Game {
        name: "Game: Special <Edition>".to_string(),
        storage_key: String::new(),
        save_paths: vec![],
        game_paths: HashMap::new(),
        next_save_unit_id: 0,
        cloud_sync_enabled: true,
        auto_backup: None,
        ludusavi_meta: None,
        device_bindings: std::collections::HashMap::new(),
    };
    let dir_name = game.backup_dir_name();
    assert!(!dir_name.contains(':'));
    assert!(!dir_name.contains('<'));
    assert!(!dir_name.contains('>'));
    assert!(!dir_name.is_empty());
}
