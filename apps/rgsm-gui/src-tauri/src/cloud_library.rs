use std::sync::Arc;

use rgsm_core::cloud_sync::CloudSyncTaskManager;
use rgsm_core::cloud_sync::v2::{
    CloudLibraryCutoverReview, CloudLibraryJoinReview, JoinGameDecision,
};
use rgsm_core::config::get_config;
use rgsm_core::services::{
    CloudLibraryCutoverOutcome, CloudLibraryJoinOutcome, CloudLibraryServiceError,
    CloudLibraryStatus, ServiceContext,
};
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
