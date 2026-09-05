use std::{future::Future, sync::Arc};

use rgsm_core::cloud_sync::v2::{
    CloudLibraryCutoverReview, CloudLibraryJoinReview, JoinGameDecision,
};
use rgsm_core::cloud_sync::{CloudSyncJobStatus, CloudSyncTaskManager};
use rgsm_core::config::{CloudNamespaceGeneration, InitialCatchUpPolicy, SyncMode, get_config};
use rgsm_core::services::{
    CloudLibraryCutoverOutcome, CloudLibraryJoinOutcome, CloudLibraryServiceError,
    CloudLibraryStatus, GameSyncModeOutcome, ServiceContext,
};
use rust_i18n::t;
use tauri::{AppHandle, Manager};

use crate::hooks::HookPipelineState;

pub async fn connect(app: &AppHandle) -> Result<CloudLibraryStatus, CloudLibraryServiceError> {
    let operation_state = app.state::<crate::cloud_operation::CloudOperationState>();
    let manager = app.state::<Arc<CloudSyncTaskManager>>();
    let generation = rgsm_core::config::cloud_namespace_generation()?;
    run_connection(&operation_state, &manager, generation, async {
        let status = ServiceContext::new(app.state::<HookPipelineState>().snapshot())
            .connect_cloud_library()
            .await?;
        crate::hooks::rebuild_pipeline(app, &get_config()?);
        Ok(status)
    })
    .await
}

async fn run_connection<T>(
    state: &crate::cloud_operation::CloudOperationState,
    manager: &CloudSyncTaskManager,
    generation: CloudNamespaceGeneration,
    operation: impl Future<Output = T>,
) -> T {
    if generation == CloudNamespaceGeneration::V2 {
        // Window creation and reload only refresh an already active library.
        return state.run(operation).await;
    }
    manager.cancel_all().await;
    state
        .run(async {
            manager.cancel_all_and_wait().await;
            operation.await
        })
        .await
}

pub async fn create(
    app: &AppHandle,
    confirmed: bool,
) -> Result<CloudLibraryStatus, CloudLibraryServiceError> {
    crate::cloud_operation::run_after_cancelling(app, async {
        let pipeline = app.state::<HookPipelineState>().snapshot();
        let status = ServiceContext::new(pipeline)
            .create_cloud_library(confirmed)
            .await?;
        let config = get_config()?;
        crate::hooks::rebuild_pipeline(app, &config);
        Ok(status)
    })
    .await
}

pub async fn rebuild(
    app: &AppHandle,
    confirmed: bool,
) -> Result<CloudLibraryStatus, CloudLibraryServiceError> {
    crate::cloud_operation::run_after_cancelling(app, async {
        let status = ServiceContext::new(app.state::<HookPipelineState>().snapshot())
            .rebuild_cloud_library_from_local(confirmed)
            .await?;
        crate::hooks::rebuild_pipeline(app, &get_config()?);
        Ok(status)
    })
    .await
}

pub async fn reconnect(
    app: &AppHandle,
    confirmed: bool,
) -> Result<CloudLibraryStatus, CloudLibraryServiceError> {
    crate::cloud_operation::run_after_cancelling(app, async {
        let status = ServiceContext::new(app.state::<HookPipelineState>().snapshot())
            .reconnect_cloud_library(confirmed)
            .await?;
        crate::hooks::rebuild_pipeline(app, &get_config()?);
        Ok(status)
    })
    .await
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
    crate::cloud_operation::run_after_cancelling(app, async {
        let outcome = ServiceContext::new(app.state::<HookPipelineState>().snapshot())
            .join_cloud_library(decisions, confirmed_replacements)
            .await?;
        if matches!(outcome, CloudLibraryJoinOutcome::Active { .. }) {
            crate::hooks::rebuild_pipeline(app, &get_config()?);
        }
        Ok(outcome)
    })
    .await
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
    crate::cloud_operation::run_after_cancelling(app, async {
        let outcome = ServiceContext::new(app.state::<HookPipelineState>().snapshot())
            .cutover_cloud_library(confirmed)
            .await?;
        crate::hooks::rebuild_pipeline(app, &get_config()?);
        Ok(outcome)
    })
    .await
}

pub async fn set_game_sync_mode(
    app: &AppHandle,
    game_id: &str,
    enabled: bool,
    mode: SyncMode,
    initial_catch_up: InitialCatchUpPolicy,
    live_save: Option<rgsm_core::services::LiveSaveSyncOptions>,
) -> Result<GameSyncModeOutcome, CloudLibraryServiceError> {
    crate::cloud_operation::run(
        app,
        set_game_sync_mode_inner(app, game_id, enabled, mode, initial_catch_up, live_save),
    )
    .await
}

async fn set_game_sync_mode_inner(
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

#[cfg(test)]
mod connection_tests {
    use super::*;
    use rgsm_core::cloud_sync::{CloudSyncError, CloudSyncStatus, SyncEventEmitter};

    struct NoopEmitter;
    impl SyncEventEmitter for NoopEmitter {
        fn emit_status(&self, _: &CloudSyncStatus) {}
        fn emit_error(&self, _: &CloudSyncError) {}
    }

    #[tokio::test]
    async fn opening_an_active_library_does_not_cancel_an_existing_transfer() {
        let manager = CloudSyncTaskManager::new(Arc::new(NoopEmitter));
        let (id, token) = manager.begin_manual_job("upload".into()).await;
        let finisher = manager.clone();
        let cancelled = token.clone();
        let task = tokio::spawn(async move {
            cancelled.cancelled().await;
            finisher
                .finish_manual_job(id, "upload", CloudSyncJobStatus::Cancelled, None)
                .await;
        });
        run_connection(
            &crate::cloud_operation::CloudOperationState::default(),
            &manager,
            CloudNamespaceGeneration::V2,
            async {},
        )
        .await;
        assert!(!token.is_cancelled());
        manager
            .finish_manual_job(id, "upload", CloudSyncJobStatus::Completed, None)
            .await;
        task.abort();
    }
}
