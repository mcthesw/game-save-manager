use crate::backup::GameSnapshots;
use crate::cloud_sync::CloudSyncSessionConfig;
use crate::cloud_sync::v2::{CloudArchiveLibraryView, DeletionRegistryRepository};
use crate::config::{cloud_bootstrap_inputs, get_config};

use super::{CloudLibraryServiceError, ServiceContext};

impl ServiceContext {
    pub async fn cloud_archive_library(
        &self,
    ) -> Result<CloudArchiveLibraryView, CloudLibraryServiceError> {
        super::game_deletion::converge_local_deleted_games().await?;
        super::retention::refresh_v2_snapshot_retention().await?;
        let (library, profile, local_state) = cloud_bootstrap_inputs()?;
        let registry = DeletionRegistryRepository::new(
            CloudSyncSessionConfig::from(&local_state.cloud_settings).get_op()?,
            3,
        )
        .load()
        .await?;
        let game_names = library
            .games
            .iter()
            .map(|game| (game.storage_key.clone(), game.name.clone()))
            .collect();
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
            .converged_materializer()
            .await?
            .view(&game_names, &local_heads)
            .await?;
        view.games
            .retain(|game| !registry.deleted_games.contains_key(&game.game_id));
        for game in &mut view.games {
            if let Some(settings) = profile.games.get(&game.game_id) {
                game.managed = true;
                game.visible = settings.visible;
                game.sync_mode = settings.sync_mode;
                game.live_save_process_name = settings.live_save_process_name.clone();
                game.live_save_snapshot_on_exit = settings.live_save_snapshot_on_exit;
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
}
