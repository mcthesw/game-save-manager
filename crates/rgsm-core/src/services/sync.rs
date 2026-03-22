use tokio_util::sync::CancellationToken;

use crate::cloud_sync::{
    BatchSyncReport, CloudSyncSessionConfig, SyncGameOutcome, download_all_from_session,
    sync_game_from_config, upload_all_from_session,
};
use crate::preclude::BackendError;

use super::ServiceContext;

impl ServiceContext {
    pub async fn check_cloud_backend(
        &self,
        session: &CloudSyncSessionConfig,
    ) -> Result<(), BackendError> {
        session.check().await
    }

    pub async fn upload_all_from_session(
        &self,
        session: &CloudSyncSessionConfig,
        token: Option<CancellationToken>,
    ) -> Result<BatchSyncReport, BackendError> {
        upload_all_from_session(session, token).await
    }

    pub async fn download_all_from_session(
        &self,
        session: &CloudSyncSessionConfig,
        token: Option<CancellationToken>,
    ) -> Result<BatchSyncReport, BackendError> {
        download_all_from_session(session, token).await
    }

    pub async fn sync_game(&self, game_name: &str) -> Result<SyncGameOutcome, BackendError> {
        sync_game_from_config(game_name).await
    }
}
