use crate::app_dirs::resolve_app_path;
use crate::backup::GameSnapshots;
use crate::cloud_sync::v2::{
    CloudArchiveLibraryView, CloudArchiveMaterializer, DeletionRegistryRepository,
};
use crate::config::{CloudNamespaceGeneration, cloud_bootstrap_inputs, get_config};

use super::{CloudLibraryServiceError, ServiceContext, cloud_library_target::bound_v2_operator};

impl ServiceContext {
    /// Reconcile cloud metadata and pending deletions at an explicit sync boundary.
    /// Archive transfer and live save restoration remain separate operations.
    pub async fn refresh_cloud_archive_library(
        &self,
    ) -> Result<CloudArchiveLibraryView, CloudLibraryServiceError> {
        super::game_deletion::converge_local_deleted_games().await?;
        super::cloud_library_metadata::refresh_shared_library().await?;
        let materializer = self.materializer().await?;
        self.converge_local_tombstone_metadata(&materializer)
            .await?;
        self.cloud_archive_library().await
    }

    /// Observe catalog state without accepting configuration, publishing presence,
    /// or deleting local archive copies.
    pub async fn cloud_archive_library(
        &self,
    ) -> Result<CloudArchiveLibraryView, CloudLibraryServiceError> {
        let (library, profile, local_state) = cloud_bootstrap_inputs()?;
        let registry = DeletionRegistryRepository::new(bound_v2_operator(&local_state).await?, 3)
            .load()
            .await?;
        let mut game_names = library
            .games
            .iter()
            .map(|game| (game.storage_key.clone(), game.name.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        for local in &local_state.local_games {
            if let Some(name) = game_names.get_mut(&local.storage_key) {
                *name = local.name.clone();
            }
        }
        let current_device = local_state.current_device_id.clone();
        let local_heads = get_config()?
            .games
            .into_iter()
            .filter_map(|game| {
                let game_id = if game.storage_key.is_empty() {
                    game.name.clone()
                } else {
                    game.storage_key.clone()
                };
                match game.get_game_snapshots_info() {
                    Ok(snapshots) => {
                        Some((game_id, snapshots.head_for_device(&current_device).cloned()))
                    }
                    Err(crate::preclude::BackupError::Io(error))
                        if error.kind() == std::io::ErrorKind::NotFound =>
                    {
                        Some((
                            game_id,
                            GameSnapshots::new(game.name)
                                .head_for_device(&current_device)
                                .cloned(),
                        ))
                    }
                    Err(_) => None,
                }
            })
            .collect();
        let mut view = self
            .materializer()
            .await?
            .view(&game_names, &local_heads)
            .await?;
        view.games
            .retain(|game| !registry.deleted_games.contains_key(&game.game_id));
        for game in &mut view.games {
            game.definition_conflict = local_state.is_local_game(&game.game_id);
            if let Some(settings) = profile.games.get(&game.game_id) {
                game.managed = true;
                game.visible = settings.visible;
                game.sync_mode = settings.sync_mode;
                game.cloud_sync_enabled = settings.cloud_sync_enabled;
                game.live_save_process_name = settings.live_save_process_name.clone();
                game.live_save_snapshot_on_exit = settings.live_save_snapshot_on_exit;
            }
            if game.definition_conflict {
                game.cloud_sync_enabled = false;
            }
            game.retention_limit = library
                .games
                .iter()
                .find(|shared| shared.storage_key == game.game_id)
                .and_then(|shared| shared.snapshot_retention)
                .map(|policy| policy.automatic_snapshots_per_branch);
        }
        Ok(view)
    }

    pub(super) async fn materializer(
        &self,
    ) -> Result<CloudArchiveMaterializer, CloudLibraryServiceError> {
        let (_, profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let local_archive_root = profile
            .local_archive_root
            .as_deref()
            .map(resolve_app_path)
            .ok_or(CloudLibraryServiceError::StorageLocationRequired)?;
        let excluded = local_state.local_game_ids();
        Ok(CloudArchiveMaterializer::new(
            bound_v2_operator(&local_state).await?,
            local_archive_root,
            local_state.current_device_id,
            resolve_app_path("GameSaveManager.cloud-v2-materialization.json"),
            3,
        )
        .excluding_games(excluded))
    }

    pub(super) async fn converged_materializer(
        &self,
    ) -> Result<CloudArchiveMaterializer, CloudLibraryServiceError> {
        super::game_deletion::converge_local_deleted_games().await?;
        let materializer = self.materializer().await?;
        self.converge_local_tombstone_metadata(&materializer)
            .await?;
        Ok(materializer)
    }

    pub(super) async fn converge_local_tombstone_metadata(
        &self,
        materializer: &CloudArchiveMaterializer,
    ) -> Result<(), CloudLibraryServiceError> {
        let tombstones = materializer.converge_local_tombstones().await?;
        if tombstones.is_empty() {
            return Ok(());
        }
        let config = get_config()?;
        for game in &config.games {
            if let Some(snapshot_ids) = tombstones.get(&game.storage_key) {
                game.forget_v2_tombstones(snapshot_ids)?;
            }
        }
        Ok(())
    }
}
