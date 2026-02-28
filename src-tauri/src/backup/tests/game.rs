use super::utils::{ConfigFileGuard, lock_config_file};
use crate::backup::archive::system_time_to_zip_datetime;
use crate::backup::state_fingerprint::{fingerprint_source_state, fingerprint_zip_state};
use crate::backup::{
    Game, GameSnapshots, SaveUnit, SaveUnitType, Snapshot, TIMER_AUTO_BACKUP_DESCRIPTION,
    TimerSnapshotDecision,
};
use crate::config::Config;
use crate::device::get_current_device_id;
use filetime::{FileTime, set_file_mtime};
use std::collections::HashMap;
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
    SaveUnit {
        unit_type: SaveUnitType::File,
        paths,
        delete_before_apply: false,
    }
}

fn init_backups_json(game_name: &str, backup_root: &Path) -> TestResult {
    let game_dir = backup_root.join(game_name);
    fs::create_dir_all(&game_dir)?;

    let snapshots = GameSnapshots {
        name: game_name.to_string(),
        backups: Vec::new(),
        head: None,
    };

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
        .filter(|snapshot| snapshot.describe == TIMER_AUTO_BACKUP_DESCRIPTION)
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
        size: fs::metadata(&zip_path)?.len(),
        parent: None,
    });
    snapshots.head = Some(date.to_string());
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
            save_paths: vec![build_file_save_unit(&save_file)],
            game_paths: HashMap::new(),
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
            save_paths: vec![build_file_save_unit(&save_file)],
            game_paths: HashMap::new(),
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
            save_paths: vec![build_file_save_unit(&save_file)],
            game_paths: HashMap::new(),
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
            save_paths: vec![build_file_save_unit(&save_file)],
            game_paths: HashMap::new(),
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
            save_paths: vec![build_file_save_unit(&save_file)],
            game_paths: HashMap::new(),
        };

        game.create_snapshot("Manual Snapshot").await?;
        let snapshots = game.get_game_snapshots_info()?;
        let latest_snapshot = snapshots
            .backups
            .last()
            .ok_or("missing latest snapshot for fingerprint test")?;

        let source_fp = fingerprint_source_state(&game.save_paths)?;
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
        size: fs::metadata(&zip_path)?.len(),
        parent: parent.map(|s| s.to_string()),
    });
    snapshots.head = Some(date.to_string());
    game.set_game_snapshots_info(&snapshots)?;
    Ok(())
}

fn make_test_game(game_name: &str, backup_root: &Path) -> Result<Game, Box<dyn std::error::Error>> {
    init_backups_json(game_name, backup_root)?;
    Ok(Game {
        name: game_name.to_string(),
        save_paths: Vec::new(),
        game_paths: HashMap::new(),
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
            result.snapshots.head.as_deref(),
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
