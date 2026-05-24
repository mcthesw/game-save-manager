use tokio_util::sync::CancellationToken;

use crate::backup::Game;
use crate::cloud_sync::{
    BatchSyncReport, CloudSyncSessionConfig, ConflictResolution, ConflictResolutionOutcome,
    SyncGameOutcome, download_all_from_session, resolve_game_conflict as resolve_cloud_conflict,
    sync_config as sync_cloud_config, sync_game as sync_cloud_game, upload_all_from_session,
};
use crate::config::{Config, get_config};
use crate::hooks::{HookSource, MetadataChangedCtx};
use crate::preclude::BackendError;

use super::ServiceContext;

fn cloud_session(config: &Config) -> CloudSyncSessionConfig {
    CloudSyncSessionConfig::from(&config.settings.cloud_settings)
}

fn find_game(config: &Config, game_name: &str) -> Result<Game, BackendError> {
    config
        .games
        .iter()
        .find(|game| game.name == game_name)
        .cloned()
        .ok_or_else(|| BackendError::GameNotFound(game_name.to_string()))
}

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
        let config = get_config()?;
        let session = cloud_session(&config);
        let op = session.get_op()?;
        let game = find_game(&config, game_name)?;
        sync_cloud_game(&session, &op, &game).await
    }

    pub async fn resolve_game_conflict(
        &self,
        game_name: &str,
        resolution: ConflictResolution,
    ) -> Result<ConflictResolutionOutcome, BackendError> {
        let config = get_config()?;
        let session = cloud_session(&config);
        let op = session.get_op()?;
        let game = find_game(&config, game_name)?;
        let outcome = resolve_cloud_conflict(&session, &op, &game, resolution).await?;

        if outcome == ConflictResolutionOutcome::AcceptedRemote {
            let snapshots = game.get_game_snapshots_info()?;
            self.pipeline()
                .fire_metadata_changed(&MetadataChangedCtx {
                    config,
                    source: HookSource::CloudSync,
                    game,
                    snapshots,
                })
                .await;
        }

        Ok(outcome)
    }

    pub async fn sync_config(&self) -> Result<(), BackendError> {
        let config = get_config()?;
        let session = cloud_session(&config);
        let op = session.get_op()?;
        sync_cloud_config(&session, &op, &config).await
    }
}
