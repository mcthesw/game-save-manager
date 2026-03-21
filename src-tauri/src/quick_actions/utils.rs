use crate::{
    backup::{CreatedBy, TIMER_AUTO_BACKUP_DESCRIPTION},
    config::{QuickActionSoundPreferences, QuickActionsSettings, get_backup_path, get_config},
    hooks::{BeforeRestoreCtx, HookSource, SnapshotAppliedCtx, SnapshotCreatedCtx},
    preclude::*,
    sound::{QuickActionSoundEffect, play_quick_action_sound},
};
use log::{error, info, warn};
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub enum QuickActionType {
    Timer,
    Tray,
    Hotkey,
}

impl QuickActionType {
    pub(super) fn generate_describe(self) -> String {
        match self {
            QuickActionType::Timer => String::from(TIMER_AUTO_BACKUP_DESCRIPTION),
            QuickActionType::Tray => String::from("Quick Backup (Tray)"),
            QuickActionType::Hotkey => String::from("Quick Backup (Hotkey)"),
        }
    }

    /// Convert to the corresponding HookSource variant.
    pub fn to_hook_source(self) -> HookSource {
        match self {
            QuickActionType::Timer => HookSource::TimerAutoBackup,
            QuickActionType::Tray => HookSource::QuickActionTray,
            QuickActionType::Hotkey => HookSource::QuickActionHotkey,
        }
    }

    /// Convert to the corresponding CreatedBy variant for snapshot metadata.
    pub fn to_created_by(self) -> CreatedBy {
        match self {
            QuickActionType::Timer => CreatedBy::Timer,
            QuickActionType::Tray => CreatedBy::Tray,
            QuickActionType::Hotkey => CreatedBy::Hotkey,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
pub enum QuickActionOperation {
    Backup,
    Apply,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
pub enum QuickActionStatus {
    Success,
    Failure,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct QuickActionCompleted {
    pub operation: QuickActionOperation,
    pub status: QuickActionStatus,
    pub trigger: QuickActionType,
    pub game_name: Option<String>,
}

pub async fn quick_apply(app: &AppHandle, t: QuickActionType) {
    info!(target:"rgsm::quick_action", "Auto apply triggered: {:#?}", t.generate_describe());
    let config = match get_config() {
        Ok(config) => config,
        Err(err) => {
            error!(target:"rgsm::quick_action", "Failed to load config: {err:?}");
            return;
        }
    };

    let quick_settings = config.quick_action.clone();
    let sound_preferences: QuickActionSoundPreferences =
        QuickActionSoundPreferences::from(&quick_settings);

    // 检查游戏是否已选择
    let game = match quick_settings.quick_action_game.clone() {
        Some(game) => game,
        None => {
            show_no_game_selected_error(app, &quick_settings, &sound_preferences);
            return;
        }
    };

    info!(target:"rgsm::quick_action", "Quick apply game: {:#?}", game);

    // 执行恢复操作
    let result = async {
        let snapshots_info = game.get_game_snapshots_info()?;
        let snapshot = snapshots_info
            .backups
            .last()
            .ok_or(BackupError::NoBackupAvailable)?
            .clone();
        let archive_path = get_backup_path()?
            .join(&game.name)
            .join(format!("{}.zip", snapshot.date));

        // Gate hooks: extra backup + integrity check
        let pipeline = app.state::<crate::hooks::HookPipelineState>().snapshot();
        pipeline
            .fire_before_restore(&BeforeRestoreCtx {
                config: config.clone(),
                source: t.to_hook_source(),
                game: game.clone(),
                snapshot: snapshot.clone(),
                snapshots: snapshots_info,
                archive_path,
            })
            .await?;

        game.restore_snapshot(&snapshot.date, None)
    }
    .await;

    // 处理结果
    match result {
        Err(e) => {
            error!(target:"rgsm::quick_action", "Quick apply failed: {:#?}", &e);
            // Failure notifications stay inline — no hook event for failures
            maybe_show_notification(
                &quick_settings,
                t!("backend.tray.error"),
                format!("{:#?}\n{:#?}", t!("backend.tray.find_error_detail"), e),
            );
            play_quick_action_sound(app, sound_preferences, QuickActionSoundEffect::Failure);
        }
        Ok(snapshots) => {
            // Fire hook pipeline — NotificationHook handles sound/notification/event
            if let Some(snapshot) = snapshots.backups.last().cloned() {
                let pipeline = app.state::<crate::hooks::HookPipelineState>().snapshot();
                pipeline
                    .fire_snapshot_applied(&SnapshotAppliedCtx {
                        config: config.clone(),
                        source: t.to_hook_source(),
                        game: game.clone(),
                        snapshot,
                        snapshots,
                    })
                    .await;
            }
        }
    }
}

pub async fn quick_backup(app: &AppHandle, t: QuickActionType) {
    info!(target:"rgsm::quick_action", "Auto backup triggered: {:#?}", t.generate_describe());
    let config = match get_config() {
        Ok(config) => config,
        Err(err) => {
            error!(target:"rgsm::quick_action", "Failed to load config: {err:?}");
            return;
        }
    };

    let quick_settings = config.quick_action.clone();
    let sound_preferences: QuickActionSoundPreferences =
        QuickActionSoundPreferences::from(&quick_settings);

    // 检查游戏是否已选择
    let game = match quick_settings.quick_action_game.clone() {
        Some(game) => game,
        None => {
            show_no_game_selected_error(app, &quick_settings, &sound_preferences);
            return;
        }
    };

    // 执行备份操作
    let result = game
        .create_snapshot_with_parent(&t.generate_describe(), None, t.to_created_by())
        .await;

    match result {
        Err(e) => {
            error!(target:"rgsm::quick_action", "Quick backup failed: {:#?}", &e);
            maybe_show_notification(
                &quick_settings,
                t!("backend.tray.error"),
                format!("{:#?}\n{:#?}", t!("backend.tray.find_error_detail"), e),
            );
            play_quick_action_sound(app, sound_preferences, QuickActionSoundEffect::Failure);
        }
        Ok(created) => {
            // Fire hook pipeline — NotificationHook handles sound/notification/event
            if let Some(snapshot) = created.snapshots.backups.last().cloned() {
                let pipeline = app.state::<crate::hooks::HookPipelineState>().snapshot();
                let mut ctx = SnapshotCreatedCtx {
                    config: config.clone(),
                    source: t.to_hook_source(),
                    game: game.clone(),
                    snapshot,
                    snapshots: created.snapshots,
                    local_zip_path: created.local_zip_path,
                    remote_zip_path: created.remote_zip_path,
                };
                pipeline.fire_snapshot_created(&mut ctx).await;
                if let Err(err) = game.set_game_snapshots_info(&ctx.snapshots) {
                    warn!(
                        target:"rgsm::quick_action",
                        "Failed to persist hook-updated snapshot metadata: {err:?}"
                    );
                }
            }
        }
    }
}

fn show_no_game_selected_error(
    app: &AppHandle,
    settings: &QuickActionsSettings,
    sound_preferences: &QuickActionSoundPreferences,
) {
    warn!(target:"rgsm::quick_action", "No game selected, cannot quick backup/apply");
    maybe_show_notification(
        settings,
        t!("backend.tray.error"),
        t!("backend.tray.no_game_selected"),
    );
    play_quick_action_sound(
        app,
        sound_preferences.clone(),
        QuickActionSoundEffect::Failure,
    );
}

fn maybe_show_notification<T1: AsRef<str>, T2: AsRef<str>>(
    settings: &QuickActionsSettings,
    title: T1,
    body: T2,
) {
    if settings.enable_notification {
        show_notification(title, body);
    }
}
