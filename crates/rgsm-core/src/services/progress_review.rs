use std::collections::BTreeMap;

use crate::app_dirs::resolve_app_path;
use crate::backup::GameSnapshots;
use crate::cloud_sync::v2::{
    CLOUD_MANIFEST_PATH, CloudManifestRepository, V2ConflictInspector, V2ConflictReview,
};
use crate::config::{CloudNamespaceGeneration, cloud_bootstrap_inputs, get_config};

use super::{CloudLibraryServiceError, ServiceContext, cloud_library_target::bound_v2_operator};

impl ServiceContext {
    /// One remote Manifest read, with independent errors for each local catalog.
    pub async fn review_v2_games_progress(
        &self,
        game_ids: &[String],
    ) -> Result<
        BTreeMap<String, Result<V2ConflictReview, CloudLibraryServiceError>>,
        CloudLibraryServiceError,
    > {
        let (library, profile, local_state) = cloud_bootstrap_inputs()?;
        if local_state.cloud_namespace_generation != CloudNamespaceGeneration::V2 {
            return Err(CloudLibraryServiceError::ActiveLibraryUnavailable);
        }
        let root = profile
            .local_archive_root
            .as_deref()
            .map(resolve_app_path)
            .ok_or(CloudLibraryServiceError::StorageLocationRequired)?;
        let operator = bound_v2_operator(&local_state).await?;
        let manifest = CloudManifestRepository::new(operator.clone(), CLOUD_MANIFEST_PATH, 3)
            .load()
            .await?;
        let inspector = V2ConflictInspector::new(operator, root, local_state.current_device_id, 3);
        let config = get_config()?;
        Ok(game_ids
            .iter()
            .map(|id| {
                let result = (|| {
                    let shared = library
                        .games
                        .iter()
                        .find(|game| game.storage_key == *id)
                        .ok_or_else(|| CloudLibraryServiceError::GameProfileNotFound(id.clone()))?;
                    let local = match config.games.iter().find(|game| game.storage_key == *id) {
                        Some(game) => match game.get_game_snapshots_info() {
                            Ok(snapshots) => snapshots,
                            Err(crate::preclude::BackupError::Io(error))
                                if error.kind() == std::io::ErrorKind::NotFound =>
                            {
                                GameSnapshots::new(&shared.name)
                            }
                            Err(error) => return Err(error.into()),
                        },
                        None => GameSnapshots::new(&shared.name),
                    };
                    Ok(inspector.review_manifest(&manifest, id, &local)?)
                })();
                (id.clone(), result)
            })
            .collect())
    }
}
