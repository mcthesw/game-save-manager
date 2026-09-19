use serde::Serialize;

use crate::backup::{Game, PendingDescription};
use crate::cloud_sync::v2::{CLOUD_MANIFEST_PATH, CloudManifestRepository, sync_descriptions};
use crate::config::{CloudNamespaceGeneration, cloud_bootstrap_inputs, get_config};
use crate::preclude::BackupError;

use super::{CloudLibraryServiceError, ServiceContext, cloud_library_target::bound_v2_operator};

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SnapshotDescriptionOutcome {
    pub cloud_sync_pending: bool,
}

impl ServiceContext {
    pub async fn set_snapshot_description(
        &self,
        requested_game: &Game,
        date: &str,
        description: &str,
    ) -> Result<SnapshotDescriptionOutcome, BackupError> {
        let config = get_config()?;
        let game = config
            .games
            .iter()
            .find(|game| game.backup_dir_name() == requested_game.backup_dir_name())
            .ok_or_else(|| BackupError::BackupNotExist {
                name: requested_game.name.clone(),
                date: date.into(),
            })?;
        let (_, profile, state) = cloud_bootstrap_inputs()?;
        let library_id = state.cloud_library_id.as_deref().filter(|_| {
            state.cloud_namespace_generation == CloudNamespaceGeneration::V2
                && !matches!(
                    state.cloud_settings.backend,
                    crate::cloud_sync::Backend::Disabled
                )
                && !state.is_local_game(&game.storage_key)
                && profile
                    .games
                    .get(&game.storage_key)
                    .is_some_and(|p| p.cloud_sync_enabled)
        });
        let mut local = game.get_game_snapshots_info()?;
        let snapshot = local
            .backups
            .iter_mut()
            .find(|s| s.date == date)
            .ok_or_else(|| BackupError::BackupNotExist {
                name: game.name.clone(),
                date: date.into(),
            })?;
        snapshot.describe = description.into();
        local.pending_descriptions.remove(date);
        if let Some(library_id) = library_id {
            local.pending_descriptions.insert(
                date.into(),
                PendingDescription {
                    library_id: library_id.into(),
                    description: description.into(),
                },
            );
        }
        // Save the text and its retry marker together, before any network access.
        game.set_game_snapshots_info(&local)?;
        if let Some(library_id) = library_id {
            let sync = async {
                let repository = CloudManifestRepository::new(
                    bound_v2_operator(&state).await?,
                    CLOUD_MANIFEST_PATH,
                    3,
                );
                sync_descriptions(&repository, library_id, &game.storage_key, &mut local).await?;
                game.set_game_snapshots_info(&local)?;
                Ok::<_, CloudLibraryServiceError>(())
            }
            .await;
            if let Err(error) = sync {
                log::warn!(
                    "Description saved locally; cloud sync pending for {}: {error}",
                    game.storage_key
                );
                return Ok(SnapshotDescriptionOutcome {
                    cloud_sync_pending: true,
                });
            }
        }
        Ok(SnapshotDescriptionOutcome {
            cloud_sync_pending: local.pending_descriptions.contains_key(date),
        })
    }
}

/// Called inside the existing cloud-operation boundary, by refresh and polling.
pub(super) async fn refresh_snapshot_descriptions() -> Result<(), CloudLibraryServiceError> {
    let (_, profile, state) = cloud_bootstrap_inputs()?;
    if state.cloud_namespace_generation != CloudNamespaceGeneration::V2
        || matches!(
            state.cloud_settings.backend,
            crate::cloud_sync::Backend::Disabled
        )
    {
        return Ok(());
    }
    let library_id = state
        .cloud_library_id
        .as_deref()
        .ok_or(CloudLibraryServiceError::ActiveLibraryUnavailable)?;
    let config = get_config()?;
    let games = config
        .games
        .iter()
        .filter(|game| {
            !state.is_local_game(&game.storage_key)
                && profile
                    .games
                    .get(&game.storage_key)
                    .is_some_and(|p| p.cloud_sync_enabled)
        })
        .collect::<Vec<_>>();
    if games.is_empty() {
        return Ok(());
    }
    let repository =
        CloudManifestRepository::new(bound_v2_operator(&state).await?, CLOUD_MANIFEST_PATH, 3);
    for game in games {
        let mut local = match game.get_game_snapshots_info() {
            Ok(local) => local,
            Err(BackupError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        let before = local.clone();
        sync_descriptions(&repository, library_id, &game.storage_key, &mut local).await?;
        if before != local {
            game.set_game_snapshots_info(&local)?;
        }
    }
    Ok(())
}
