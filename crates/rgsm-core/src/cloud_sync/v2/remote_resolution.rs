use std::collections::HashSet;
use std::path::PathBuf;

use opendal::Operator;
use thiserror::Error;

use super::materialization::verify_file;
use super::{
    CLOUD_MANIFEST_PATH, CloudArchiveMaterializer, CloudManifestRepository, DeletionRegistryError,
    DeletionRegistryRepository, ManifestError, ManifestRepositoryError, MaterializationError,
    OpenDalManifestTransport, SnapshotState,
};
use crate::backup::{GameSnapshots, Snapshot, archive_path, snapshot_archive_path};
use crate::device::DeviceId;

pub struct PreparedRemoteProgress {
    pub selected_snapshot_id: String,
    pub lineage: Vec<Snapshot>,
}

pub struct V2RemoteProgressResolver {
    operator: Operator,
    local_archive_root: PathBuf,
    current_device_id: DeviceId,
    progress_path: PathBuf,
    max_attempts: usize,
}

impl V2RemoteProgressResolver {
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

    pub async fn prepare(
        &self,
        game_id: &str,
        expected_manifest_revision: u64,
        expected_local_snapshot_id: Option<&str>,
        selected_snapshot_id: &str,
        local: &GameSnapshots,
    ) -> Result<PreparedRemoteProgress, AcceptRemoteProgressError> {
        let actual_local_head = local.head_for_device(&self.current_device_id).cloned();
        if actual_local_head.as_deref() != expected_local_snapshot_id {
            return Err(AcceptRemoteProgressError::LocalPositionChanged {
                expected: expected_local_snapshot_id.map(str::to_string),
                actual: actual_local_head,
            });
        }

        let repository = self.repository();
        let initial = repository.load().await?;
        if initial.revision != expected_manifest_revision {
            return Err(AcceptRemoteProgressError::StaleReview {
                expected: expected_manifest_revision,
                actual: initial.revision,
            });
        }
        let game = initial
            .games
            .get(game_id)
            .ok_or_else(|| AcceptRemoteProgressError::GameNotFound(game_id.to_string()))?;
        ensure_remote_candidate(game, &self.current_device_id, selected_snapshot_id, true)?;
        let selected = game
            .snapshots
            .get(selected_snapshot_id)
            .ok_or_else(|| ManifestError::MissingSnapshot(selected_snapshot_id.to_string()))?;
        let SnapshotState::Live(live) = &selected.state else {
            return Err(AcceptRemoteProgressError::CandidateNoLongerAdvertised(
                selected_snapshot_id.to_string(),
            ));
        };
        let integrity = live.integrity.clone().ok_or_else(|| {
            AcceptRemoteProgressError::SelectedArchiveUnavailable(selected_snapshot_id.to_string())
        })?;
        let game_archive_root = self.local_archive_root.join(game_id);
        let local_path = local
            .backups
            .iter()
            .find(|snapshot| snapshot.date == selected_snapshot_id)
            .map(|snapshot| snapshot_archive_path(&game_archive_root, snapshot))
            .unwrap_or_else(|| {
                archive_path(
                    &game_archive_root,
                    selected_snapshot_id,
                    selected.archive_format,
                )
            });
        if tokio::fs::try_exists(&local_path).await?
            && verify_file(integrity, local_path).await.is_err()
        {
            return Err(AcceptRemoteProgressError::LocalArchiveConflict(
                selected_snapshot_id.to_string(),
            ));
        }

        CloudArchiveMaterializer::new(
            self.operator.clone(),
            self.local_archive_root.clone(),
            self.current_device_id.clone(),
            self.progress_path.clone(),
            self.max_attempts,
        )
        .download(game_id, selected_snapshot_id)
        .await?;

        let prepared = repository.load().await?;
        let game = prepared
            .games
            .get(game_id)
            .ok_or_else(|| AcceptRemoteProgressError::GameNotFound(game_id.to_string()))?;
        ensure_remote_candidate(game, &self.current_device_id, selected_snapshot_id, false)?;
        Ok(PreparedRemoteProgress {
            selected_snapshot_id: selected_snapshot_id.to_string(),
            lineage: remote_lineage(
                game_id,
                game,
                selected_snapshot_id,
                &self.local_archive_root,
            )?,
        })
    }

    pub async fn commit_current_device_head(
        &self,
        game_id: &str,
        selected_snapshot_id: &str,
    ) -> Result<u64, AcceptRemoteProgressError> {
        DeletionRegistryRepository::new(self.operator.clone(), self.max_attempts)
            .ensure_active(&self.current_device_id, game_id)
            .await?;
        let game_id = game_id.to_string();
        let selected_snapshot_id = selected_snapshot_id.to_string();
        let current_device_id = self.current_device_id.clone();
        let stored =
            self.repository()
                .mutate(move |manifest| {
                    let game = manifest
                        .games
                        .get_mut(&game_id)
                        .ok_or_else(|| ManifestError::MissingGame(game_id.clone()))?;
                    let selected = game.snapshots.get(&selected_snapshot_id).ok_or_else(|| {
                        ManifestError::MissingSnapshot(selected_snapshot_id.clone())
                    })?;
                    if !matches!(
                        &selected.state,
                        SnapshotState::Live(live) if live.cloud_archive_verified
                    ) || !game.device_heads.iter().any(|(device, head)| {
                        device != &current_device_id && head == &selected_snapshot_id
                    }) {
                        return Err(ManifestError::InvalidHead {
                            device: current_device_id.clone(),
                            snapshot: selected_snapshot_id.clone(),
                        });
                    }
                    game.set_head(current_device_id.clone(), selected_snapshot_id.clone());
                    Ok(())
                })
                .await?;
        Ok(stored.revision)
    }

    fn repository(&self) -> CloudManifestRepository<OpenDalManifestTransport> {
        CloudManifestRepository::new(
            self.operator.clone(),
            CLOUD_MANIFEST_PATH,
            self.max_attempts,
        )
    }
}

fn ensure_remote_candidate(
    game: &super::GameManifest,
    current_device_id: &str,
    selected_snapshot_id: &str,
    require_cloud_archive: bool,
) -> Result<(), AcceptRemoteProgressError> {
    if !game
        .device_heads
        .iter()
        .any(|(device, head)| device != current_device_id && head == selected_snapshot_id)
    {
        return Err(AcceptRemoteProgressError::CandidateNoLongerAdvertised(
            selected_snapshot_id.to_string(),
        ));
    }
    let selected = game
        .snapshots
        .get(selected_snapshot_id)
        .ok_or_else(|| ManifestError::MissingSnapshot(selected_snapshot_id.to_string()))?;
    let SnapshotState::Live(live) = &selected.state else {
        return Err(AcceptRemoteProgressError::CandidateNoLongerAdvertised(
            selected_snapshot_id.to_string(),
        ));
    };
    if require_cloud_archive && !live.cloud_archive_verified {
        return Err(AcceptRemoteProgressError::SelectedArchiveUnavailable(
            selected_snapshot_id.to_string(),
        ));
    }
    Ok(())
}

/// Build one selected parent chain in parent-first order.
///
/// Each node is visited once. The cycle guard makes the traversal O(V) time
/// and O(V) additional space for V Snapshots on the selected lineage.
fn remote_lineage(
    game_id: &str,
    game: &super::GameManifest,
    selected_snapshot_id: &str,
    local_archive_root: &std::path::Path,
) -> Result<Vec<Snapshot>, AcceptRemoteProgressError> {
    let mut lineage = Vec::new();
    let mut visited = HashSet::new();
    let mut cursor = selected_snapshot_id;
    loop {
        if !visited.insert(cursor.to_string()) {
            return Err(AcceptRemoteProgressError::ParentCycle(cursor.to_string()));
        }
        let node = game
            .snapshots
            .get(cursor)
            .ok_or_else(|| ManifestError::MissingSnapshot(cursor.to_string()))?;
        let SnapshotState::Live(live) = &node.state else {
            return Err(AcceptRemoteProgressError::TombstonedLineage(
                cursor.to_string(),
            ));
        };
        lineage.push(Snapshot {
            date: node.snapshot_id.clone(),
            describe: node.description.clone(),
            path: archive_path(
                &local_archive_root.join(game_id),
                &node.snapshot_id,
                node.archive_format,
            )
            .to_string_lossy()
            .into_owned(),
            archive_format: node.archive_format,
            size: live.integrity.as_ref().map_or(0, |value| value.size),
            parent: node.parent.clone(),
            archive_hash: live.integrity.as_ref().map(|value| value.xxh3_64.clone()),
            device_id: None,
            created_by: live.created_by.clone(),
        });
        let Some(parent) = node.parent.as_deref() else {
            break;
        };
        cursor = parent;
    }
    lineage.reverse();
    Ok(lineage)
}

#[derive(Debug, Error)]
pub enum AcceptRemoteProgressError {
    #[error("Cloud progress review is stale: expected revision {expected}, found {actual}")]
    StaleReview { expected: u64, actual: u64 },
    #[error("Local Current Position changed: expected {expected:?}, found {actual:?}")]
    LocalPositionChanged {
        expected: Option<String>,
        actual: Option<String>,
    },
    #[error("Game does not exist in the Cloud Library: {0}")]
    GameNotFound(String),
    #[error("Selected progress is no longer advertised by another Device: {0}")]
    CandidateNoLongerAdvertised(String),
    #[error("Selected Snapshot Archive is not available in the cloud: {0}")]
    SelectedArchiveUnavailable(String),
    #[error("A different Local Archive already uses the selected Snapshot identity: {0}")]
    LocalArchiveConflict(String),
    #[error("Selected progress contains a deleted ancestor: {0}")]
    TombstonedLineage(String),
    #[error("Cloud Snapshot parent cycle contains {0}")]
    ParentCycle(String),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Materialization(#[from] MaterializationError),
    #[error(transparent)]
    Repository(#[from] ManifestRepositoryError),
    #[error(transparent)]
    DeletionRegistry(#[from] DeletionRegistryError),
    #[error("Local Archive inspection failed: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use opendal::services;

    use super::*;
    use crate::backup::{ArchiveFormat, CreatedBy};
    use crate::cloud_sync::v2::{
        ArchiveIntegrity, CloudManifest, GameManifest, SnapshotNode, cloud_archive_path,
    };

    async fn fixture() -> (
        Operator,
        temp_dir::TempDir,
        V2RemoteProgressResolver,
        GameSnapshots,
    ) {
        let operator = Operator::new(services::Memory::default()).unwrap().finish();
        let root = temp_dir::TempDir::new().unwrap();
        let mut game = GameManifest::new("game");
        let mut root_node = SnapshotNode::live(
            "root",
            None,
            ArchiveIntegrity {
                size: 4,
                xxh3_64: "89b1c2d328a57a3c".into(),
            },
            CreatedBy::Manual,
        );
        let mut remote = SnapshotNode::live(
            "remote",
            Some("root".into()),
            ArchiveIntegrity {
                size: 6,
                xxh3_64: "8353b2824506ea9b".into(),
            },
            CreatedBy::Manual,
        );
        if let SnapshotState::Live(live) = &mut root_node.state {
            live.cloud_archive_verified = true;
        }
        if let SnapshotState::Live(live) = &mut remote.state {
            live.cloud_archive_verified = true;
        }
        game.upsert_live(root_node).unwrap();
        game.upsert_live(remote).unwrap();
        game.set_head("deck".into(), "remote".into());
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
        operator
            .write(
                &cloud_archive_path("game", "remote", ArchiveFormat::Zip).unwrap(),
                b"remote".to_vec(),
            )
            .await
            .unwrap();
        let local = GameSnapshots::new("Game");
        let resolver = V2RemoteProgressResolver::new(
            operator.clone(),
            root.path().join("archives"),
            "pc".into(),
            root.path().join("progress.json"),
            2,
        );
        (operator, root, resolver, local)
    }

    #[tokio::test]
    async fn downloads_selected_candidate_and_preserves_its_lineage() {
        let (_operator, _root, resolver, local) = fixture().await;

        let prepared = resolver
            .prepare("game", 7, None, "remote", &local)
            .await
            .unwrap();

        assert_eq!(prepared.selected_snapshot_id, "remote");
        assert_eq!(
            prepared
                .lineage
                .iter()
                .map(|snapshot| snapshot.date.as_str())
                .collect::<Vec<_>>(),
            vec!["root", "remote"]
        );
        assert!(std::path::Path::new(&prepared.lineage[1].path).is_file());
    }

    #[tokio::test]
    async fn stale_review_does_not_download_or_mutate_manifest() {
        let (_operator, _root, resolver, local) = fixture().await;

        assert!(matches!(
            resolver.prepare("game", 6, None, "remote", &local).await,
            Err(AcceptRemoteProgressError::StaleReview {
                expected: 6,
                actual: 7
            })
        ));
        assert_eq!(resolver.repository().load().await.unwrap().revision, 7);
    }

    #[tokio::test]
    async fn conflicting_local_archive_is_not_overwritten() {
        let (_operator, root, resolver, mut local) = fixture().await;
        let local_path = root.path().join("relocated/remote.zip");
        local.backups.push(Snapshot {
            date: "remote".into(),
            describe: "local collision".into(),
            path: local_path.to_string_lossy().into_owned(),
            archive_format: ArchiveFormat::Zip,
            size: 6,
            parent: Some("root".into()),
            archive_hash: None,
            device_id: Some("pc".into()),
            created_by: CreatedBy::Manual,
        });
        std::fs::create_dir_all(local_path.parent().unwrap()).unwrap();
        std::fs::write(&local_path, b"local!").unwrap();

        assert!(matches!(
            resolver.prepare("game", 7, None, "remote", &local).await,
            Err(AcceptRemoteProgressError::LocalArchiveConflict(snapshot))
                if snapshot == "remote"
        ));
        assert_eq!(std::fs::read(local_path).unwrap(), b"local!");
    }

    #[tokio::test]
    async fn commit_moves_only_the_current_device_head() {
        let (_operator, _root, resolver, local) = fixture().await;
        resolver
            .prepare("game", 7, None, "remote", &local)
            .await
            .unwrap();

        resolver
            .commit_current_device_head("game", "remote")
            .await
            .unwrap();

        let manifest = resolver.repository().load().await.unwrap();
        assert_eq!(manifest.games["game"].device_heads["pc"], "remote");
        assert_eq!(manifest.games["game"].device_heads["deck"], "remote");
    }
}
