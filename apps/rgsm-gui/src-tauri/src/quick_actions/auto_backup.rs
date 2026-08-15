use log::{info, warn};
use tauri::{AppHandle, Manager};

use rgsm_core::backup::{AutoBackupConfig, Game, TimerSnapshotDecision};
use rgsm_core::config::get_config;
use rgsm_core::hooks::SnapshotDeletedCtx;
use rgsm_core::services::ServiceContext;

use super::{QuickActionType, notify_backup_failed, notify_backup_skipped_unchanged};

pub async fn perform_changed_auto_backup(
    app: &AppHandle,
    game: &Game,
    retention: Option<&AutoBackupConfig>,
    trigger: QuickActionType,
) {
    let describe = trigger.generate_describe();
    let created_by = trigger.to_created_by();

    let service = ServiceContext::new(app.state::<crate::hooks::HookPipelineState>().snapshot());
    match service
        .create_snapshot_if_changed(game, &describe, created_by, trigger.to_hook_source(), None)
        .await
    {
        Ok(TimerSnapshotDecision::SkippedUnchanged) => {
            info!(
                target: "rgsm::quick_action::auto_backup",
                "Skipped automatic backup for '{}': state unchanged",
                game.name
            );
            if let Ok(config) = get_config() {
                notify_backup_skipped_unchanged(app, &config, trigger, &game.name);
            }
            return;
        }
        Ok(TimerSnapshotDecision::Created) => {
            info!(
                target: "rgsm::quick_action::auto_backup",
                "Automatic backup created for '{}'",
                game.name
            );
        }
        Err(err) => {
            warn!(
                target: "rgsm::quick_action::auto_backup",
                "Automatic backup failed for '{}': {err:?}",
                game.name
            );
            if let Ok(config) = get_config() {
                notify_backup_failed(app, &config, trigger, &game.name, &err.to_string());
            }
            return;
        }
    }

    let config = match get_config() {
        Ok(config) => config,
        Err(err) => {
            warn!(
                target: "rgsm::quick_action::auto_backup",
                "Failed to load config after automatic backup: {err:?}"
            );
            return;
        }
    };

    cleanup_old_auto_backups(app, game, retention, config, trigger).await;
}

async fn cleanup_old_auto_backups(
    app: &AppHandle,
    game: &Game,
    retention: Option<&AutoBackupConfig>,
    config: rgsm_core::config::Config,
    trigger: QuickActionType,
) {
    let effective_max = retention
        .and_then(|retention| retention.max_backup_count)
        .unwrap_or(config.settings.max_auto_backup_count);

    if effective_max == 0 {
        return;
    }

    match game.cleanup_old_auto_backups(effective_max).await {
        Ok(cleanup_result) => {
            if !cleanup_result.deleted_remote_paths.is_empty() {
                let pipeline = app.state::<crate::hooks::HookPipelineState>().snapshot();
                pipeline
                    .fire_snapshot_deleted(&SnapshotDeletedCtx {
                        config,
                        source: trigger.to_hook_source(),
                        game: game.clone(),
                        snapshots: cleanup_result.snapshots,
                        deleted_remote_paths: cleanup_result.deleted_remote_paths,
                    })
                    .await;
            }
        }
        Err(err) => {
            warn!(
                target: "rgsm::quick_action::auto_backup",
                "Automatic backup cleanup failed for '{}': {err:?}",
                game.name
            );
        }
    }
}
