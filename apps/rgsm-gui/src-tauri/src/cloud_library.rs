use std::sync::Arc;

use rgsm_core::cloud_sync::CloudSyncTaskManager;
use rgsm_core::config::get_config;
use rgsm_core::services::{CloudLibraryServiceError, CloudLibraryStatus, ServiceContext};
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
