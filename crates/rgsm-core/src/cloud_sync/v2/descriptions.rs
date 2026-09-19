use crate::backup::GameSnapshots;

use super::{CloudManifestRepository, ManifestRepositoryError, ManifestTransport, SnapshotState};

/// Publish only explicit edits, then accept shared descriptions for local copies.
/// No archive, ancestry, provenance, or device-presence metadata is changed.
pub(crate) async fn sync_descriptions<T: ManifestTransport>(
    repository: &CloudManifestRepository<T>,
    library_id: &str,
    game_id: &str,
    local: &mut GameSnapshots,
) -> Result<(), ManifestRepositoryError> {
    let mut manifest = repository.load().await?;
    let updates = local
        .pending_descriptions
        .iter()
        .filter(|(id, edit)| {
            edit.library_id == library_id && local.backups.iter().any(|s| &s.date == *id)
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let publishable = manifest.games.get(game_id).is_some_and(|game| {
        updates.keys().any(|id| {
            game.snapshots
                .get(*id)
                .is_some_and(|node| matches!(node.state, SnapshotState::Live(_)))
        })
    });
    if publishable {
        manifest = repository
            .mutate(|manifest| {
                if let Some(game) = manifest.games.get_mut(game_id) {
                    for (id, edit) in &updates {
                        if let Some(node) = game.snapshots.get_mut(*id)
                            && matches!(node.state, SnapshotState::Live(_))
                        {
                            node.description.clone_from(&edit.description);
                        }
                    }
                }
                Ok(())
            })
            .await?;
    }
    // Clear edits only after a successful remote write/read. A missing cloud
    // node remains pending until normal snapshot publication introduces it.
    local.pending_descriptions.retain(|id, edit| {
        edit.library_id == library_id
            && local.backups.iter().any(|s| s.date == *id)
            && manifest
                .games
                .get(game_id)
                .and_then(|g| g.snapshots.get(id))
                .is_none()
    });
    if let Some(game) = manifest.games.get(game_id) {
        for snapshot in &mut local.backups {
            if let Some(node) = game.snapshots.get(&snapshot.date)
                && matches!(node.state, SnapshotState::Live(_))
            {
                snapshot.describe.clone_from(&node.description);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
