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
        let local = game.update_game_snapshots_info::<BackupError>(|local| {
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
            Ok(())
        })?;
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
            game.update_game_snapshots_info::<BackupError>(|current| {
                merge_synced_descriptions(current, &before, &local);
                Ok(())
            })?;
        }
    }
    Ok(())
}

/// Apply only description results whose local input still matches. Never replace
/// ancestry, current position, new snapshots, or a newer unsent description.
fn merge_synced_descriptions(
    current: &mut crate::backup::GameSnapshots,
    before: &crate::backup::GameSnapshots,
    synced: &crate::backup::GameSnapshots,
) {
    for snapshot in &mut current.backups {
        let id = &snapshot.date;
        let Some(previous) = before.backups.iter().find(|old| old.date == *id) else {
            continue;
        };
        if snapshot.describe != previous.describe
            || current.pending_descriptions.get(id) != before.pending_descriptions.get(id)
        {
            continue;
        }
        let Some(result) = synced.backups.iter().find(|result| result.date == *id) else {
            continue;
        };
        snapshot.describe.clone_from(&result.describe);
        if let Some(pending) = synced.pending_descriptions.get(id) {
            current
                .pending_descriptions
                .insert(id.clone(), pending.clone());
        } else {
            current.pending_descriptions.remove(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{GameSnapshots, PendingDescription};

    fn fixture() -> GameSnapshots {
        let mut snapshots = GameSnapshots::new("game");
        snapshots.backups.push(
            serde_json::from_value(serde_json::json!({
                "date": "one", "describe": "old", "path": "one.zip"
            }))
            .unwrap(),
        );
        snapshots.pending_descriptions.insert(
            "one".into(),
            PendingDescription {
                library_id: "library".into(),
                description: "old".into(),
            },
        );
        snapshots
    }

    #[test]
    fn background_description_result_preserves_new_local_work() {
        let before = fixture();
        let mut synced = before.clone();
        synced.pending_descriptions.clear();
        let mut current = before.clone();
        current.backups[0].describe = "newer".into();
        current
            .pending_descriptions
            .get_mut("one")
            .unwrap()
            .description = "newer".into();
        current.backups.push(
            serde_json::from_value(serde_json::json!({
                "date": "two", "describe": "new snapshot", "path": "two.zip"
            }))
            .unwrap(),
        );
        current.set_current_device_head(Some("two".into()));
        let expected = current.clone();
        merge_synced_descriptions(&mut current, &before, &synced);
        assert_eq!(current, expected);
    }

    #[test]
    fn background_description_result_clears_only_the_published_edit() {
        let before = fixture();
        let mut synced = before.clone();
        synced.pending_descriptions.clear();
        let mut current = before.clone();
        merge_synced_descriptions(&mut current, &before, &synced);
        assert!(current.pending_descriptions.is_empty());
        current.backups.clear();
        merge_synced_descriptions(&mut current, &before, &synced);
        assert!(current.backups.is_empty());
    }
}
