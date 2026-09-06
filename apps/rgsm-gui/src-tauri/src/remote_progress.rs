//! Application orchestration for progress reminders; the notice book owns deduplication.
use std::{collections::BTreeSet, sync::Mutex};

use rgsm_core::{
    cloud_sync::v2::CloudArchiveLibraryView,
    services::{CloudLibraryServiceError, ServiceContext},
};
use tauri::{AppHandle, Manager};

use crate::progress_notices::{PendingProgress, ProgressNoticeBook, should_notify};

pub struct RemoteProgressState {
    book: Mutex<ProgressNoticeBook>,
    desktop_enabled: bool,
}

impl RemoteProgressState {
    pub fn new(desktop_enabled: bool) -> Self {
        Self {
            book: Mutex::default(),
            desktop_enabled,
        }
    }
}

/// Caller serializes this read with cloud connection changes and mutations.
pub async fn refresh(
    app: &AppHandle,
    service: &ServiceContext,
) -> Result<CloudArchiveLibraryView, CloudLibraryServiceError> {
    let view = service.refresh_cloud_archive_library().await?;
    let eligible = view
        .games
        .iter()
        .filter(|game| {
            game.managed
                && game.cloud_sync_enabled
                && !game.definition_conflict
                && game.sync_mode.checks_remote_progress()
        })
        .map(|game| game.game_id.clone())
        .collect::<BTreeSet<_>>();
    let reviews = if eligible.is_empty() {
        Ok(Default::default())
    } else {
        service
            .review_v2_games_progress(&eligible.iter().cloned().collect::<Vec<_>>())
            .await
    };
    let state = app.state::<RemoteProgressState>();
    let window_focused = app.get_webview_window("main").is_some_and(|window| {
        window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false)
    });
    let mut book = state.book.lock().expect("progress notices poisoned");
    book.retain_games(&view.library_id, &eligible);
    let mut new_progress = false;
    match reviews {
        Ok(reviews) => {
            for (id, result) in reviews {
                match result {
                    Ok(review) => {
                        let game = view
                            .games
                            .iter()
                            .find(|game| game.game_id == id)
                            .expect("eligible game");
                        new_progress |= book.observe(
                            &game.name,
                            rgsm_core::device::get_current_device_id(),
                            &review,
                        );
                    }
                    Err(error) => log::warn!("Cannot inspect remote progress for {id}: {error}"),
                }
            }
        }
        Err(error) => log::warn!("Cannot inspect remote progress: {error}"),
    }
    let pending = book.pending();
    // Publish while holding the book lock so a concurrent defer cannot emit older state last.
    crate::http::emit(app, "remote-progress-pending", &pending);
    drop(book);
    if should_notify(state.desktop_enabled, window_focused, new_progress) {
        rgsm_core::preclude::show_notification(
            rust_i18n::t!("sync_settings.archives.progress.pending_title"),
            rust_i18n::t!("sync_settings.archives.progress.background_notice"),
        );
    }
    Ok(view)
}

pub fn defer(app: &AppHandle, ids: &[String]) -> PendingProgress {
    let state = app.state::<RemoteProgressState>();
    let mut book = state.book.lock().expect("progress notices poisoned");
    book.defer(ids);
    let pending = book.pending();
    crate::http::emit(app, "remote-progress-pending", &pending);
    pending
}
