use crate::cloud_sync::v2::CloudArchiveLibraryView;
use crate::config::cloud_bootstrap_inputs;

use super::{CloudLibraryServiceError, ServiceContext};

impl ServiceContext {
    pub async fn cloud_archive_library(
        &self,
    ) -> Result<CloudArchiveLibraryView, CloudLibraryServiceError> {
        super::retention::refresh_v2_snapshot_retention().await?;
        let (library, profile, _) = cloud_bootstrap_inputs()?;
        let game_names = library
            .games
            .iter()
            .map(|game| (game.storage_key.clone(), game.name.clone()))
            .collect();
        let mut view = self
            .converged_materializer()
            .await?
            .view(&game_names)
            .await?;
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
