//! Built-in hook for user-facing notifications, sounds, and frontend events.
//!
//! It runs late in the pipeline so visible feedback reflects already-committed
//! lifecycle changes.

use anyhow::Result;
use async_trait::async_trait;
use log::{info, warn};
use tauri::AppHandle;

use crate::config::QuickActionSoundPreferences;
use crate::preclude::show_notification;
use crate::quick_actions::{
    QuickActionCompleted, QuickActionOperation, QuickActionStatus, QuickActionType,
};
use crate::sound::{QuickActionSoundEffect, play_quick_action_sound};

use super::pipeline::{HookSource, SnapshotAppliedCtx, SnapshotCreatedCtx, SnapshotHook};

/// Plays sounds, shows system notifications, and emits frontend events
/// for quick-action / timer sources.
///
/// Priority 90 — runs last so all data mutations are already committed.
pub struct NotificationHook {
    app: AppHandle,
}

impl NotificationHook {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    /// Map HookSource to QuickActionType. Returns None for non-quick-action sources.
    fn to_quick_action_type(source: &HookSource) -> Option<QuickActionType> {
        match source {
            HookSource::TimerAutoBackup => Some(QuickActionType::Timer),
            HookSource::QuickActionHotkey => Some(QuickActionType::Hotkey),
            HookSource::QuickActionTray => Some(QuickActionType::Tray),
            _ => None,
        }
    }

    fn notify_success(
        &self,
        config: &crate::config::Config,
        qa_type: QuickActionType,
        operation: QuickActionOperation,
        game_name: &str,
    ) {
        let quick_settings = &config.quick_action;
        let sound_prefs = QuickActionSoundPreferences::from(quick_settings);

        // System notification (timer respects prompt_when_auto_backup)
        let should_notify = match qa_type {
            QuickActionType::Timer => config.settings.prompt_when_auto_backup,
            _ => true,
        };
        if quick_settings.enable_notification && should_notify {
            let op_key = match operation {
                QuickActionOperation::Backup => rust_i18n::t!("backend.tray.quick_backup"),
                QuickActionOperation::Apply => rust_i18n::t!("backend.tray.quick_apply"),
            };
            show_notification(
                rust_i18n::t!("backend.tray.success"),
                format!(
                    "{:#?} {} {}",
                    game_name,
                    op_key,
                    rust_i18n::t!("backend.tray.success")
                ),
            );
        }

        // Sound
        play_quick_action_sound(&self.app, sound_prefs, QuickActionSoundEffect::Success);

        // Frontend event
        emit_quick_action_event(
            &self.app,
            qa_type,
            operation,
            QuickActionStatus::Success,
            Some(game_name.to_string()),
        );
    }
}

fn emit_quick_action_event(
    app: &AppHandle,
    trigger: QuickActionType,
    operation: QuickActionOperation,
    status: QuickActionStatus,
    game_name: Option<String>,
) {
    use tauri_specta::Event;
    if let Err(err) = (QuickActionCompleted {
        operation,
        status,
        trigger,
        game_name,
    })
    .emit(app)
    {
        warn!(
            target: "rgsm::hooks::notification",
            "Failed to emit quick action event: {err:?}"
        );
    }
}

#[async_trait]
impl SnapshotHook for NotificationHook {
    fn name(&self) -> &str {
        "NotificationHook"
    }

    fn priority(&self) -> u32 {
        90
    }

    async fn on_snapshot_created(&self, ctx: &mut SnapshotCreatedCtx) -> Result<()> {
        if let Some(qa_type) = Self::to_quick_action_type(&ctx.source) {
            info!(
                target: "rgsm::hooks::notification",
                "Snapshot created via {:?} for {} — sending notification",
                ctx.source, ctx.game.name
            );
            self.notify_success(
                &ctx.config,
                qa_type,
                QuickActionOperation::Backup,
                &ctx.game.name,
            );
        }
        Ok(())
    }

    async fn on_snapshot_applied(&self, ctx: &SnapshotAppliedCtx) -> Result<()> {
        if let Some(qa_type) = Self::to_quick_action_type(&ctx.source) {
            info!(
                target: "rgsm::hooks::notification",
                "Snapshot applied via {:?} for {} — sending notification",
                ctx.source, ctx.game.name
            );
            self.notify_success(
                &ctx.config,
                qa_type,
                QuickActionOperation::Apply,
                &ctx.game.name,
            );
        }
        Ok(())
    }
}
