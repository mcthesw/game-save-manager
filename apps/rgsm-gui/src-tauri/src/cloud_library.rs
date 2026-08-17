use std::sync::Arc;

use rgsm_core::cloud_sync::v2::{
    CloudLibraryCutoverReview, CloudLibraryJoinReview, JoinGameDecision,
};
use rgsm_core::cloud_sync::{CloudSyncJobStatus, CloudSyncTaskManager};
use rgsm_core::config::{InitialCatchUpPolicy, SyncMode, get_config};
use rgsm_core::services::{
    CloudLibraryCutoverOutcome, CloudLibraryJoinOutcome, CloudLibraryServiceError,
    CloudLibraryStatus, GameSyncModeOutcome, ServiceContext,
};
use rust_i18n::t;
use tauri::{AppHandle, Manager};

use crate::hooks::HookPipelineState;

pub async fn create(
    app: &AppHandle,
    confirmed: bool,
) -> Result<CloudLibraryStatus, CloudLibraryServiceError> {
    app.state::<Arc<CloudSyncTaskManager>>()
        .cancel_all_and_wait()
        .await;
    let pipeline = app.state::<HookPipelineState>().snapshot();
    let status = ServiceContext::new(pipeline)
        .create_cloud_library(confirmed)
        .await?;
    let config = get_config()?;
    crate::hooks::rebuild_pipeline(app, &config);
    Ok(status)
}

pub async fn review(app: &AppHandle) -> Result<CloudLibraryJoinReview, CloudLibraryServiceError> {
    ServiceContext::new(app.state::<HookPipelineState>().snapshot())
        .review_cloud_library_join()
        .await
}

pub async fn join(
    app: &AppHandle,
    decisions: &[JoinGameDecision],
    confirmed_replacements: bool,
) -> Result<CloudLibraryJoinOutcome, CloudLibraryServiceError> {
    app.state::<Arc<CloudSyncTaskManager>>()
        .cancel_all_and_wait()
        .await;
    let outcome = ServiceContext::new(app.state::<HookPipelineState>().snapshot())
        .join_cloud_library(decisions, confirmed_replacements)
        .await?;
    if matches!(outcome, CloudLibraryJoinOutcome::Active { .. }) {
        crate::hooks::rebuild_pipeline(app, &get_config()?);
    }
    Ok(outcome)
}

pub async fn review_cutover(
    app: &AppHandle,
) -> Result<CloudLibraryCutoverReview, CloudLibraryServiceError> {
    ServiceContext::new(app.state::<HookPipelineState>().snapshot())
        .review_cloud_library_cutover()
        .await
}

pub async fn cutover(
    app: &AppHandle,
    confirmed: bool,
) -> Result<CloudLibraryCutoverOutcome, CloudLibraryServiceError> {
    app.state::<Arc<CloudSyncTaskManager>>()
        .cancel_all_and_wait()
        .await;
    let outcome = ServiceContext::new(app.state::<HookPipelineState>().snapshot())
        .cutover_cloud_library(confirmed)
        .await?;
    crate::hooks::rebuild_pipeline(app, &get_config()?);
    Ok(outcome)
}

pub async fn set_game_sync_mode(
    app: &AppHandle,
    game_id: &str,
    enabled: bool,
    mode: SyncMode,
    initial_catch_up: InitialCatchUpPolicy,
    live_save: Option<rgsm_core::services::LiveSaveSyncOptions>,
) -> Result<GameSyncModeOutcome, CloudLibraryServiceError> {
    let manager = Arc::clone(app.state::<Arc<CloudSyncTaskManager>>().inner());
    let description = t!("backend.sync.updating_mode", game = game_id).to_string();
    let (job_id, token) = manager.begin_manual_job(description.clone()).await;
    let outcome = ServiceContext::new(app.state::<HookPipelineState>().snapshot())
        .set_game_cloud_policy(game_id, enabled, mode, initial_catch_up, live_save, &token)
        .await;
    let status = if outcome.is_ok() {
        CloudSyncJobStatus::Completed
    } else if token.is_cancelled() {
        CloudSyncJobStatus::Cancelled
    } else {
        CloudSyncJobStatus::Failed
    };
    manager
        .finish_manual_job(
            job_id,
            &description,
            status,
            outcome.as_ref().err().map(ToString::to_string),
        )
        .await;
    crate::hooks::rebuild_pipeline(app, &get_config()?);
    if outcome.is_ok()
        && let Some(monitor) = app.try_state::<crate::quick_actions::ProcessMonitor>()
    {
        monitor.sync_from_config();
    }
    outcome
}
