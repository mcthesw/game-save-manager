use std::{collections::HashMap, fs, path::Path, sync::Arc};

use crate::{
    backup::{Game, SaveUnit, SaveUnitSource, SaveUnitType},
    config::Config,
    device::get_current_device_id,
    hooks::{ArchiveVerifyHook, HookPipeline, HookSource, PreRestoreBackupHook},
    preclude::BackupError,
    services::ServiceContext,
};

use super::utils::{ConfigFileGuard, lock_config_file};

fn game_for(path: &Path) -> Game {
    let paths = HashMap::from([(
        get_current_device_id().clone(),
        path.to_string_lossy().into_owned(),
    )]);
    serde_json::from_value(serde_json::json!({
        "name": "Safety",
        "storage_key": "safety-game",
        "save_paths": [SaveUnit {
            id: 1,
            source: SaveUnitSource::Concrete { unit_type: SaveUnitType::File, paths },
            enabled: true,
            delete_before_apply: true,
        }],
        "next_save_unit_id": 2,
    }))
    .unwrap()
}

fn service() -> ServiceContext {
    ServiceContext::new(Arc::new(HookPipeline::new(vec![
        Box::new(PreRestoreBackupHook),
        Box::new(ArchiveVerifyHook),
    ])))
}

struct Fixture {
    temp: temp_dir::TempDir,
    game: Game,
    save: std::path::PathBuf,
    snapshot_date: String,
    _guard: ConfigFileGuard,
}

async fn fixture() -> Result<Fixture, Box<dyn std::error::Error>> {
    let temp = temp_dir::TempDir::new()?;
    let save = temp.path().join("save.sav");
    fs::write(&save, "snapshot-state")?;
    let game = game_for(&save);
    let mut config = Config {
        backup_path: temp.path().join("backups").to_string_lossy().into_owned(),
        games: vec![game.clone()],
        ..Config::default()
    };
    config.settings.extra_backup_when_apply = true;
    config.settings.max_extra_backup_count = 1;
    let guard = ConfigFileGuard::write_config(&config)?;
    service()
        .create_snapshot(&game, "", HookSource::UserManual, None)
        .await?;
    let snapshot_date = game.get_game_snapshots_info()?.backups[0].date.clone();
    fs::write(&save, "live-state")?;
    Ok(Fixture {
        temp,
        game,
        save,
        snapshot_date,
        _guard: guard,
    })
}

fn extra_backup_dir(fixture: &Fixture) -> std::path::PathBuf {
    fixture
        .temp
        .path()
        .join("backups")
        .join(fixture.game.backup_dir_name().as_ref())
        .join("extra_backup")
}

#[test]
fn failed_safety_backup_keeps_live_save() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = lock_config_file();
    tokio::runtime::Builder::new_current_thread()
        .build()?
        .block_on(async {
            let fixture = fixture().await?;
            fs::write(extra_backup_dir(&fixture), "not a directory")?;

            let error = service()
                .restore_snapshot(
                    &fixture.game,
                    &fixture.snapshot_date,
                    HookSource::UserManual,
                    None,
                )
                .await
                .unwrap_err();

            assert!(matches!(error, BackupError::PreRestoreBackupFailed(_)));
            assert_eq!(fs::read_to_string(&fixture.save)?, "live-state");
            Ok(())
        })
}

#[test]
fn unresolved_save_location_still_asks_for_selection() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = lock_config_file();
    tokio::runtime::Builder::new_current_thread()
        .build()?
        .block_on(async {
            let fixture = fixture().await?;
            let mut broken = fixture.game.clone();
            broken.save_paths[0].source = SaveUnitSource::Concrete {
                unit_type: SaveUnitType::File,
                paths: HashMap::new(),
            };

            let error = service()
                .restore_snapshot(
                    &broken,
                    &fixture.snapshot_date,
                    HookSource::UserManual,
                    None,
                )
                .await
                .unwrap_err();

            assert!(matches!(
                error,
                BackupError::RestorePlan(crate::backup::RestorePlanError::SelectionRequired { .. })
            ));
            assert_eq!(fs::read_to_string(&fixture.save)?, "live-state");
            Ok(())
        })
}

#[test]
fn missing_live_save_restores_without_safety_backup() -> Result<(), Box<dyn std::error::Error>> {
    let _lock = lock_config_file();
    tokio::runtime::Builder::new_current_thread()
        .build()?
        .block_on(async {
            let fixture = fixture().await?;
            fs::remove_file(&fixture.save)?;

            service()
                .restore_snapshot(
                    &fixture.game,
                    &fixture.snapshot_date,
                    HookSource::UserManual,
                    None,
                )
                .await?;

            assert_eq!(fs::read_to_string(&fixture.save)?, "snapshot-state");
            Ok(())
        })
}

#[test]
fn corrupt_archive_is_rejected_before_safety_backup_evicts_older_one()
-> Result<(), Box<dyn std::error::Error>> {
    let _lock = lock_config_file();
    tokio::runtime::Builder::new_current_thread()
        .build()?
        .block_on(async {
            let fixture = fixture().await?;
            let backups = fixture.game.get_game_snapshots_info()?;
            let snapshot = backups.backups[0].clone();
            let archive = crate::backup::snapshot_archive_path(
                &crate::config::get_backup_path()?.join(fixture.game.backup_dir_name().as_ref()),
                &snapshot,
            );
            fixture
                .game
                .update_game_snapshots_info::<BackupError>(|current| {
                    current.backups[0].archive_hash =
                        Some(crate::backup::compute_file_hash(&archive)?);
                    Ok(())
                })?;
            let extra = extra_backup_dir(&fixture);
            fs::create_dir_all(&extra)?;
            let existing = extra.join("Overwrite_old.7z");
            fs::write(&existing, "older safety backup")?;
            fs::write(&archive, "corrupted")?;

            let error = service()
                .restore_snapshot(
                    &fixture.game,
                    &fixture.snapshot_date,
                    HookSource::UserManual,
                    None,
                )
                .await
                .unwrap_err();

            assert!(matches!(error, BackupError::IntegrityCheckFailed { .. }));
            assert!(existing.exists());
            assert_eq!(fs::read_dir(&extra)?.count(), 1);
            Ok(())
        })
}
