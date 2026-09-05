use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use opendal::Operator;
use serde::Serialize;
use specta::Type;
use thiserror::Error;

use super::{
    CLOUD_MANIFEST_PATH, CloudArchiveMaterializer, CloudManifestRepository, DeletionRegistryError,
    DeletionRegistryRepository, ManifestError, ManifestRepositoryError, MaterializationError,
    OpenDalManifestTransport, SnapshotState, SnapshotSyncCoordinator, SnapshotSyncError,
};
use crate::backup::{GameSnapshots, Snapshot};
use crate::device::DeviceId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type, utoipa::ToSchema)]
pub struct KeepLocalProgressOutcome {
    pub snapshot_id: String,
    pub prepared_snapshots: usize,
    pub uploaded_archives: usize,
    pub manifest_revision: u64,
}

pub struct V2ConflictResolver {
    operator: Operator,
    local_archive_root: PathBuf,
    current_device_id: DeviceId,
    progress_path: PathBuf,
    max_attempts: usize,
}

impl V2ConflictResolver {
    pub fn new(
        operator: Operator,
        local_archive_root: PathBuf,
        current_device_id: DeviceId,
        progress_path: PathBuf,
        max_attempts: usize,
    ) -> Self {
        Self {
            operator,
            local_archive_root,
            current_device_id,
            progress_path,
            max_attempts: max_attempts.max(1),
        }
    }

    pub async fn keep_local(
        &self,
        game_id: &str,
        expected_manifest_revision: u64,
        expected_local_snapshot_id: &str,
        local: &GameSnapshots,
    ) -> Result<KeepLocalProgressOutcome, KeepLocalProgressError> {
        DeletionRegistryRepository::new(self.operator.clone(), self.max_attempts)
            .ensure_active(&self.current_device_id, game_id)
            .await?;
        let actual_local_head = local.head_for_device(&self.current_device_id).cloned();
        if actual_local_head.as_deref() != Some(expected_local_snapshot_id) {
            return Err(KeepLocalProgressError::LocalPositionChanged {
                expected: expected_local_snapshot_id.to_string(),
                actual: actual_local_head,
            });
        }

        let repository = self.repository();
        let initial = repository.load().await?;
        if initial.revision != expected_manifest_revision {
            return Err(KeepLocalProgressError::StaleReview {
                expected: expected_manifest_revision,
                actual: initial.revision,
            });
        }
        if !initial.games.contains_key(game_id) {
            return Err(KeepLocalProgressError::GameNotFound(game_id.to_string()));
        }

        let history = local_history(local, expected_local_snapshot_id)?;
        let other_games = initial
            .games
            .keys()
            .filter(|id| id.as_str() != game_id)
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let coordinator = SnapshotSyncCoordinator::new(
            self.operator.clone(),
            self.local_archive_root.clone(),
            self.current_device_id.clone(),
            self.progress_path.clone(),
            self.max_attempts,
        )
        .excluding_games(other_games.clone());
        for snapshot in &history {
            coordinator
                .publish_local_node(game_id, snapshot, None)
                .await?;
        }

        let materializer = CloudArchiveMaterializer::new(
            self.operator.clone(),
            self.local_archive_root.clone(),
            self.current_device_id.clone(),
            self.progress_path.clone(),
            self.max_attempts,
        )
        .excluding_games(other_games);
        let mut uploaded_archives = 0;
        for snapshot in &history {
            let manifest = repository.load().await?;
            let cloud_verified = manifest
                .games
                .get(game_id)
                .and_then(|game| game.snapshots.get(&snapshot.date))
                .is_some_and(|node| {
                    matches!(
                        &node.state,
                        SnapshotState::Live(live) if live.cloud_archive_verified
                    )
                });
            if !cloud_verified && coordinator.local_path(game_id, snapshot).is_file() {
                materializer.upload(game_id, &snapshot.date).await?;
                uploaded_archives += 1;
            }
        }

        let prepared = repository.load().await?;
        let selected_is_available = prepared
            .games
            .get(game_id)
            .and_then(|game| game.snapshots.get(expected_local_snapshot_id))
            .is_some_and(|node| {
                matches!(
                    &node.state,
                    SnapshotState::Live(live) if live.cloud_archive_verified
                )
            });
        if !selected_is_available {
            return Err(KeepLocalProgressError::SelectedArchiveUnavailable(
                expected_local_snapshot_id.to_string(),
            ));
        }
        DeletionRegistryRepository::new(self.operator.clone(), self.max_attempts)
            .ensure_active(&self.current_device_id, game_id)
            .await?;

        let game_id = game_id.to_string();
        let selected_snapshot_id = expected_local_snapshot_id.to_string();
        let current_device_id = self.current_device_id.clone();
        let stored = repository
            .mutate(move |manifest| {
                let game = manifest
                    .games
                    .get_mut(&game_id)
                    .ok_or_else(|| ManifestError::MissingGame(game_id.clone()))?;
                let selected = game
                    .snapshots
                    .get(&selected_snapshot_id)
                    .ok_or_else(|| ManifestError::MissingSnapshot(selected_snapshot_id.clone()))?;
                if !matches!(
                    &selected.state,
                    SnapshotState::Live(live) if live.cloud_archive_verified
                ) {
                    return Err(ManifestError::InvalidIntegrity(
                        selected_snapshot_id.clone(),
                    ));
                }
                game.set_head(current_device_id.clone(), selected_snapshot_id.clone());
                Ok(())
            })
            .await?;

        Ok(KeepLocalProgressOutcome {
            snapshot_id: expected_local_snapshot_id.to_string(),
            prepared_snapshots: history.len(),
            uploaded_archives,
            manifest_revision: stored.revision,
        })
    }

    fn repository(&self) -> CloudManifestRepository<OpenDalManifestTransport> {
        CloudManifestRepository::new(
            self.operator.clone(),
            CLOUD_MANIFEST_PATH,
            self.max_attempts,
        )
    }
}

/// Return the selected local Head and all of its ancestors in parent-first order.
///
/// The walk follows exactly one parent edge per Snapshot. The visited set makes
/// malformed cycles fail closed, giving O(V) time and O(V) additional space for
/// V Snapshots on the selected lineage.
fn local_history(
    local: &GameSnapshots,
    selected_snapshot_id: &str,
) -> Result<Vec<Snapshot>, KeepLocalProgressError> {
    let snapshots = local
        .backups
        .iter()
        .map(|snapshot| (snapshot.date.as_str(), snapshot))
        .collect::<BTreeMap<_, _>>();
    let mut lineage = Vec::new();
    let mut visited = HashSet::new();
    let mut cursor = selected_snapshot_id;
    loop {
        if !visited.insert(cursor.to_string()) {
            return Err(KeepLocalProgressError::LocalParentCycle(cursor.to_string()));
        }
        let snapshot = snapshots
            .get(cursor)
            .ok_or_else(|| KeepLocalProgressError::MissingLocalSnapshot(cursor.to_string()))?;
        lineage.push((*snapshot).clone());
        let Some(parent) = snapshot.parent.as_deref() else {
            break;
        };
        cursor = parent;
    }
    lineage.reverse();
    Ok(lineage)
}

#[derive(Debug, Error)]
pub enum KeepLocalProgressError {
    #[error("Cloud progress review is stale: expected revision {expected}, found {actual}")]
    StaleReview { expected: u64, actual: u64 },
    #[error("Local Current Position changed: expected {expected}, found {actual:?}")]
    LocalPositionChanged {
        expected: String,
        actual: Option<String>,
    },
    #[error("Game does not exist in the Cloud Library: {0}")]
    GameNotFound(String),
    #[error("Local progress is missing Snapshot {0}")]
    MissingLocalSnapshot(String),
    #[error("Local Snapshot parent cycle contains {0}")]
    LocalParentCycle(String),
    #[error("Selected local Snapshot Archive is not available locally or in the cloud: {0}")]
    SelectedArchiveUnavailable(String),
    #[error(transparent)]
    SnapshotSync(#[from] SnapshotSyncError),
    #[error(transparent)]
    Materialization(#[from] MaterializationError),
    #[error(transparent)]
    Repository(#[from] ManifestRepositoryError),
    #[error(transparent)]
    DeletionRegistry(#[from] DeletionRegistryError),
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use opendal::services;

    use super::*;
    use crate::backup::{ArchiveFormat, CreatedBy, archive_path};
    use crate::cloud_sync::v2::{
        ArchiveIntegrity, CloudManifest, GameManifest, SnapshotNode, cloud_archive_path,
    };

    fn snapshot(id: &str, parent: Option<&str>) -> Snapshot {
        Snapshot {
            date: id.into(),
            describe: id.into(),
            path: String::new(),
            archive_format: ArchiveFormat::Zip,
            size: 0,
            parent: parent.map(str::to_string),
            archive_hash: None,
            device_id: Some("pc".into()),
            created_by: CreatedBy::Manual,
        }
    }

    fn node(id: &str) -> SnapshotNode {
        SnapshotNode::live(
            id,
            None,
            ArchiveIntegrity {
                size: 6,
                xxh3_64: "0000000000000000".into(),
            },
            CreatedBy::Manual,
        )
    }

    async fn fixture() -> (
        Operator,
        temp_dir::TempDir,
        V2ConflictResolver,
        GameSnapshots,
    ) {
        let operator = Operator::new(services::Memory::default()).unwrap().finish();
        let root = temp_dir::TempDir::new().unwrap();
        let archive_root = root.path().join("archives");
        let game_root = archive_root.join("game");
        std::fs::create_dir_all(&game_root).unwrap();
        std::fs::write(
            archive_path(&game_root, "root", ArchiveFormat::Zip),
            b"root",
        )
        .unwrap();
        std::fs::write(
            archive_path(&game_root, "local", ArchiveFormat::Zip),
            b"local",
        )
        .unwrap();
        let mut game = GameManifest::new("game");
        game.upsert_live(node("remote")).unwrap();
        game.set_head("deck".into(), "remote".into());
        game.set_head("pc".into(), "remote".into());
        operator
            .write(
                CLOUD_MANIFEST_PATH,
                serde_json::to_vec_pretty(&CloudManifest {
                    revision: 7,
                    games: BTreeMap::from([("game".into(), game)]),
                    ..Default::default()
                })
                .unwrap(),
            )
            .await
            .unwrap();
        let mut local = GameSnapshots::new("Game");
        local.backups = vec![snapshot("root", None), snapshot("local", Some("root"))];
        local.set_head_for_device("pc".into(), Some("local".into()));
        let resolver = V2ConflictResolver::new(
            operator.clone(),
            archive_root,
            "pc".into(),
            root.path().join("progress.json"),
            2,
        );
        (operator, root, resolver, local)
    }

    #[tokio::test]
    async fn uploads_selected_lineage_before_moving_only_current_device_head() {
        let (operator, _root, resolver, local) = fixture().await;

        let outcome = resolver
            .keep_local("game", 7, "local", &local)
            .await
            .unwrap();

        assert_eq!(outcome.prepared_snapshots, 2);
        assert_eq!(outcome.uploaded_archives, 2);
        let manifest = resolver.repository().load().await.unwrap();
        let game = &manifest.games["game"];
        assert_eq!(game.device_heads["pc"], "local");
        assert_eq!(game.device_heads["deck"], "remote");
        assert!(game.snapshots.get("local").is_some_and(
            |node| matches!(&node.state, SnapshotState::Live(live) if live.cloud_archive_verified)
        ));
        assert!(
            operator
                .exists(&cloud_archive_path("game", "local", ArchiveFormat::Zip).unwrap())
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn stale_review_fails_before_any_manifest_mutation() {
        let (_operator, _root, resolver, local) = fixture().await;

        assert!(matches!(
            resolver.keep_local("game", 6, "local", &local).await,
            Err(KeepLocalProgressError::StaleReview {
                expected: 6,
                actual: 7
            })
        ));
        let manifest = resolver.repository().load().await.unwrap();
        assert_eq!(manifest.revision, 7);
        assert_eq!(manifest.games["game"].device_heads["pc"], "remote");
    }

    #[tokio::test]
    async fn unavailable_selected_archive_never_moves_the_device_head() {
        let (_operator, _root, resolver, local) = fixture().await;
        std::fs::remove_file(archive_path(
            &resolver.local_archive_root.join("game"),
            "local",
            ArchiveFormat::Zip,
        ))
        .unwrap();

        assert!(matches!(
            resolver.keep_local("game", 7, "local", &local).await,
            Err(KeepLocalProgressError::SelectedArchiveUnavailable(id)) if id == "local"
        ));
        let manifest = resolver.repository().load().await.unwrap();
        assert_eq!(manifest.games["game"].device_heads["pc"], "remote");
        assert_eq!(manifest.games["game"].device_heads["deck"], "remote");
    }
}
