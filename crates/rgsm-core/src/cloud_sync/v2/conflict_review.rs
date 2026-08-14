use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use opendal::Operator;
use serde::Serialize;
use specta::Type;
use thiserror::Error;

use super::{
    CLOUD_MANIFEST_PATH, CloudManifestRepository, ManifestError, ManifestRepositoryError,
    SnapshotState,
};
use crate::backup::{GameSnapshots, Snapshot, archive_path};
use crate::device::DeviceId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct V2ConflictReview {
    pub game_id: String,
    pub manifest_revision: u64,
    pub local: Option<LocalProgressView>,
    pub candidates: Vec<RemoteProgressCandidate>,
    pub requires_choice: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct LocalProgressView {
    pub snapshot_id: String,
    pub description: String,
    pub local_available: bool,
    pub cloud_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct RemoteProgressCandidate {
    pub snapshot_id: String,
    pub description: String,
    pub devices: Vec<DeviceId>,
    pub relation: ProgressRelation,
    pub local_unique_snapshots: usize,
    pub remote_unique_snapshots: usize,
    pub common_ancestor: Option<String>,
    pub local_available: bool,
    pub cloud_available: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProgressRelation {
    Same,
    RemoteAhead,
    RemoteEarlier,
    DifferentProgress,
    NoLocalPosition,
}

pub struct V2ConflictInspector {
    operator: Operator,
    local_archive_root: PathBuf,
    current_device_id: DeviceId,
    max_attempts: usize,
}

impl V2ConflictInspector {
    pub fn new(
        operator: Operator,
        local_archive_root: PathBuf,
        current_device_id: DeviceId,
        max_attempts: usize,
    ) -> Self {
        Self {
            operator,
            local_archive_root,
            current_device_id,
            max_attempts: max_attempts.max(1),
        }
    }

    pub async fn review(
        &self,
        game_id: &str,
        local_snapshots: &GameSnapshots,
    ) -> Result<V2ConflictReview, ConflictReviewError> {
        let manifest = CloudManifestRepository::new(
            self.operator.clone(),
            CLOUD_MANIFEST_PATH,
            self.max_attempts,
        )
        .load()
        .await?;
        let game = manifest
            .games
            .get(game_id)
            .ok_or_else(|| ConflictReviewError::GameNotFound(game_id.to_string()))?;
        let graph = SnapshotGraph::from_sources(game, local_snapshots)?;
        let local_head = local_snapshots
            .head_for_device(&self.current_device_id)
            .cloned();
        if let Some(head) = &local_head {
            graph.validate_chain(head)?;
            if game
                .snapshots
                .get(head)
                .is_some_and(|node| !node.state.is_live())
            {
                return Err(ConflictReviewError::TombstonedLocalPosition(head.clone()));
            }
        }

        let local = local_head.as_ref().map(|snapshot_id| {
            let node = game.snapshots.get(snapshot_id);
            LocalProgressView {
                snapshot_id: snapshot_id.clone(),
                description: local_snapshots
                    .backups
                    .iter()
                    .find(|snapshot| snapshot.date == *snapshot_id)
                    .map(|snapshot| snapshot.describe.clone())
                    .or_else(|| node.map(|node| node.description.clone()))
                    .unwrap_or_default(),
                local_available: local_snapshots
                    .backups
                    .iter()
                    .find(|snapshot| snapshot.date == *snapshot_id)
                    .is_some_and(|snapshot| {
                        archive_path(
                            &self.local_archive_root.join(game_id),
                            snapshot_id,
                            snapshot.archive_format,
                        )
                        .is_file()
                    }),
                cloud_available: node.is_some_and(cloud_available),
            }
        });

        let mut grouped_heads = BTreeMap::<String, BTreeSet<DeviceId>>::new();
        for (device, head) in &game.device_heads {
            grouped_heads
                .entry(head.clone())
                .or_default()
                .insert(device.clone());
        }
        let requires_choice = progress_requires_choice(local_head.as_deref(), &grouped_heads);
        let mut candidates = Vec::with_capacity(grouped_heads.len());
        for (snapshot_id, devices) in grouped_heads {
            let node = game
                .snapshots
                .get(&snapshot_id)
                .ok_or_else(|| ManifestError::MissingSnapshot(snapshot_id.clone()))?;
            let diff = graph.compare(local_head.as_deref(), &snapshot_id)?;
            candidates.push(RemoteProgressCandidate {
                snapshot_id: snapshot_id.clone(),
                description: node.description.clone(),
                devices: devices.into_iter().collect(),
                relation: diff.relation,
                local_unique_snapshots: diff.local_unique,
                remote_unique_snapshots: diff.remote_unique,
                common_ancestor: diff.common_ancestor,
                local_available: self.local_verified(game_id, game, node),
                cloud_available: cloud_available(node),
            });
        }

        Ok(V2ConflictReview {
            game_id: game_id.to_string(),
            manifest_revision: manifest.revision,
            local,
            candidates,
            requires_choice,
        })
    }

    fn local_verified(
        &self,
        game_id: &str,
        game: &super::GameManifest,
        node: &super::SnapshotNode,
    ) -> bool {
        let SnapshotState::Live(live) = &node.state else {
            return false;
        };
        let Some(integrity) = &live.integrity else {
            return false;
        };
        game.local_archives
            .get(&self.current_device_id)
            .is_some_and(|items| items.contains(&node.snapshot_id))
            && file_size_matches(
                &archive_path(
                    &self.local_archive_root.join(game_id),
                    &node.snapshot_id,
                    node.archive_format,
                ),
                integrity.size,
            )
    }
}

pub fn progress_requires_choice(
    local_head: Option<&str>,
    advertised_heads: &BTreeMap<String, BTreeSet<DeviceId>>,
) -> bool {
    advertised_heads.len() > 1
        || local_head.is_some_and(|head| !advertised_heads.contains_key(head))
        || (local_head.is_none() && !advertised_heads.is_empty())
}

fn cloud_available(node: &super::SnapshotNode) -> bool {
    matches!(&node.state, SnapshotState::Live(live) if live.cloud_archive_verified)
}

fn file_size_matches(path: &Path, expected: u64) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.len() == expected)
}

struct SnapshotGraph {
    parents: BTreeMap<String, Option<String>>,
}

struct ProgressDiff {
    relation: ProgressRelation,
    local_unique: usize,
    remote_unique: usize,
    common_ancestor: Option<String>,
}

impl SnapshotGraph {
    fn from_sources(
        game: &super::GameManifest,
        local: &GameSnapshots,
    ) -> Result<Self, ConflictReviewError> {
        game.validate()?;

        let mut graph = Self::from_manifest(game);
        for snapshot in &local.backups {
            graph.merge_local_snapshot(game, snapshot)?;
        }

        graph.validate()?;
        Ok(graph)
    }

    fn from_manifest(game: &super::GameManifest) -> Self {
        Self {
            parents: game
                .snapshots
                .iter()
                .map(|(id, node)| (id.clone(), node.parent.clone()))
                .collect(),
        }
    }

    fn merge_local_snapshot(
        &mut self,
        game: &super::GameManifest,
        snapshot: &Snapshot,
    ) -> Result<(), ConflictReviewError> {
        let Some(existing_parent) = self.parents.get(&snapshot.date) else {
            self.parents
                .insert(snapshot.date.clone(), snapshot.parent.clone());
            return Ok(());
        };
        let manifest_snapshot = game.snapshots.get(&snapshot.date);
        if shared_snapshot_matches(existing_parent, manifest_snapshot, snapshot) {
            return Ok(());
        }
        Err(ConflictReviewError::SnapshotIdentityConflict(
            snapshot.date.clone(),
        ))
    }

    fn validate(&self) -> Result<(), ConflictReviewError> {
        for snapshot_id in self.parents.keys() {
            self.validate_chain(snapshot_id)?;
        }
        Ok(())
    }

    /// Compare two parent chains by their first shared ancestor.
    ///
    /// Each chain is visited once, so time and additional space are O(V) for
    /// V nodes reachable from the two Heads.
    fn compare(
        &self,
        local_head: Option<&str>,
        remote_head: &str,
    ) -> Result<ProgressDiff, ConflictReviewError> {
        let Some(local_head) = local_head else {
            return Ok(ProgressDiff {
                relation: ProgressRelation::NoLocalPosition,
                local_unique: 0,
                remote_unique: self.chain(remote_head)?.len(),
                common_ancestor: None,
            });
        };
        let local_chain = self.chain(local_head)?;
        let remote_chain = self.chain(remote_head)?;
        let remote_positions = remote_chain
            .iter()
            .enumerate()
            .map(|(index, id)| (id.as_str(), index))
            .collect::<BTreeMap<_, _>>();
        let shared = local_chain
            .iter()
            .enumerate()
            .find_map(|(local_index, id)| {
                remote_positions
                    .get(id.as_str())
                    .map(|remote_index| (local_index, *remote_index, id.clone()))
            });
        let (local_unique, remote_unique, common_ancestor) =
            shared.unwrap_or((local_chain.len(), remote_chain.len(), String::new()));
        let relation = match (local_unique, remote_unique) {
            (0, 0) => ProgressRelation::Same,
            (0, _) => ProgressRelation::RemoteAhead,
            (_, 0) => ProgressRelation::RemoteEarlier,
            _ => ProgressRelation::DifferentProgress,
        };
        Ok(ProgressDiff {
            relation,
            local_unique,
            remote_unique,
            common_ancestor: (!common_ancestor.is_empty()).then_some(common_ancestor),
        })
    }

    fn validate_chain(&self, start: &str) -> Result<(), ConflictReviewError> {
        self.chain(start).map(|_| ())
    }

    fn chain(&self, start: &str) -> Result<Vec<String>, ConflictReviewError> {
        let mut chain = Vec::new();
        let mut cursor = start;
        let mut visited = HashSet::new();
        loop {
            if !visited.insert(cursor.to_string()) {
                return Err(ConflictReviewError::ParentCycle(cursor.to_string()));
            }
            chain.push(cursor.to_string());
            let parent = self
                .parents
                .get(cursor)
                .ok_or_else(|| ConflictReviewError::MissingSnapshot(cursor.to_string()))?;
            let Some(parent) = parent.as_deref() else {
                return Ok(chain);
            };
            cursor = parent;
        }
    }
}

fn shared_snapshot_matches(
    existing_parent: &Option<String>,
    manifest_snapshot: Option<&super::SnapshotNode>,
    local_snapshot: &Snapshot,
) -> bool {
    existing_parent == &local_snapshot.parent
        && manifest_snapshot
            .is_none_or(|node| manifest_snapshot_matches_local(node, local_snapshot))
}

fn manifest_snapshot_matches_local(
    manifest_snapshot: &super::SnapshotNode,
    local_snapshot: &Snapshot,
) -> bool {
    if manifest_snapshot.archive_format != local_snapshot.archive_format {
        return false;
    }
    let SnapshotState::Live(live) = &manifest_snapshot.state else {
        return true;
    };
    live.created_by == local_snapshot.created_by
        && live
            .integrity
            .as_ref()
            .is_none_or(|integrity| local_integrity_matches(local_snapshot, integrity))
}

fn local_integrity_matches(
    local_snapshot: &Snapshot,
    manifest_integrity: &super::ArchiveIntegrity,
) -> bool {
    (local_snapshot.size == 0 || manifest_integrity.size == local_snapshot.size)
        && local_snapshot
            .archive_hash
            .as_ref()
            .is_none_or(|hash| hash == &manifest_integrity.xxh3_64)
}

#[derive(Debug, Error)]
pub enum ConflictReviewError {
    #[error("Game does not exist in the Cloud Library: {0}")]
    GameNotFound(String),
    #[error("Snapshot does not exist in the combined progress graph: {0}")]
    MissingSnapshot(String),
    #[error("Snapshot parent cycle contains {0}")]
    ParentCycle(String),
    #[error("Local and Cloud progress disagree about Snapshot identity: {0}")]
    SnapshotIdentityConflict(String),
    #[error("The local Current Position was deleted from shared history: {0}")]
    TombstonedLocalPosition(String),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Repository(#[from] ManifestRepositoryError),
}

#[cfg(test)]
mod tests {
    use opendal::services;

    use super::*;
    use crate::backup::{ArchiveFormat, CreatedBy, Snapshot};
    use crate::cloud_sync::v2::{ArchiveIntegrity, CloudManifest, GameManifest, SnapshotNode};

    fn snapshot(id: &str, parent: Option<&str>) -> Snapshot {
        Snapshot {
            date: id.into(),
            describe: id.into(),
            path: String::new(),
            archive_format: ArchiveFormat::Zip,
            size: 0,
            parent: parent.map(str::to_string),
            archive_hash: None,
            device_id: None,
            created_by: CreatedBy::Manual,
        }
    }

    fn node(id: &str, parent: Option<&str>) -> SnapshotNode {
        SnapshotNode::live(
            id,
            parent.map(str::to_string),
            ArchiveIntegrity {
                size: 1,
                xxh3_64: "0000000000000000".into(),
            },
            CreatedBy::Manual,
        )
    }

    async fn inspector(game: GameManifest, root: &Path) -> V2ConflictInspector {
        let operator = Operator::new(services::Memory::default()).unwrap().finish();
        operator
            .write(
                CLOUD_MANIFEST_PATH,
                serde_json::to_vec(&CloudManifest {
                    revision: 7,
                    games: BTreeMap::from([("game".into(), game)]),
                    ..Default::default()
                })
                .unwrap(),
            )
            .await
            .unwrap();
        V2ConflictInspector::new(operator, root.to_path_buf(), "pc".into(), 2)
    }

    #[test]
    fn shared_snapshot_identity_compares_portable_metadata() {
        let manifest_snapshot = node("same", None);
        let mut local_snapshot = snapshot("same", None);
        local_snapshot.size = 1;
        local_snapshot.archive_hash = Some("0000000000000000".into());

        assert!(manifest_snapshot_matches_local(
            &manifest_snapshot,
            &local_snapshot
        ));

        local_snapshot.size = 2;
        assert!(!manifest_snapshot_matches_local(
            &manifest_snapshot,
            &local_snapshot
        ));

        local_snapshot.size = 1;
        local_snapshot.archive_hash = Some("different".into());
        assert!(!manifest_snapshot_matches_local(
            &manifest_snapshot,
            &local_snapshot
        ));

        local_snapshot.archive_hash = Some("0000000000000000".into());
        local_snapshot.created_by = CreatedBy::Timer;
        assert!(!manifest_snapshot_matches_local(
            &manifest_snapshot,
            &local_snapshot
        ));

        local_snapshot.created_by = CreatedBy::Manual;
        local_snapshot.archive_format = ArchiveFormat::SevenZ;
        assert!(!manifest_snapshot_matches_local(
            &manifest_snapshot,
            &local_snapshot
        ));
    }

    #[tokio::test]
    async fn groups_devices_and_keeps_comparable_heads_as_choices() {
        let root = temp_dir::TempDir::new().unwrap();
        let mut game = GameManifest::new("game");
        for entry in [
            node("root", None),
            node("local", Some("root")),
            node("remote-a", Some("local")),
            node("remote-b", Some("remote-a")),
        ] {
            game.upsert_live(entry).unwrap();
        }
        game.set_head("deck".into(), "remote-a".into());
        game.set_head("laptop".into(), "remote-a".into());
        game.set_head("handheld".into(), "remote-b".into());
        let mut local = GameSnapshots::new("Game");
        local.backups = vec![snapshot("root", None), snapshot("local", Some("root"))];
        local.set_head_for_device("pc".into(), Some("local".into()));

        let review = inspector(game, root.path())
            .await
            .review("game", &local)
            .await
            .unwrap();

        assert!(review.requires_choice);
        assert_eq!(review.candidates.len(), 2);
        assert_eq!(review.candidates[0].devices, vec!["deck", "laptop"]);
        assert_eq!(review.candidates[0].relation, ProgressRelation::RemoteAhead);
        assert_eq!(review.candidates[0].remote_unique_snapshots, 1);
        assert_eq!(review.candidates[1].remote_unique_snapshots, 2);
    }

    #[tokio::test]
    async fn reports_earlier_divergent_and_headless_relations() {
        let root = temp_dir::TempDir::new().unwrap();
        let mut game = GameManifest::new("game");
        for entry in [
            node("root", None),
            node("local", Some("root")),
            node("local-tip", Some("local")),
            node("earlier", Some("root")),
            node("other", Some("root")),
        ] {
            game.upsert_live(entry).unwrap();
        }
        game.set_head("old".into(), "root".into());
        game.set_head("other".into(), "other".into());
        let inspector = inspector(game, root.path()).await;
        let mut local = GameSnapshots::new("Game");
        local.backups = vec![
            snapshot("root", None),
            snapshot("local", Some("root")),
            snapshot("local-tip", Some("local")),
        ];
        local.set_head_for_device("pc".into(), Some("local-tip".into()));

        let review = inspector.review("game", &local).await.unwrap();

        assert_eq!(
            review.candidates[0].relation,
            ProgressRelation::DifferentProgress
        );
        assert_eq!(
            review.candidates[1].relation,
            ProgressRelation::RemoteEarlier
        );
        let headless = inspector
            .review("game", &GameSnapshots::new("Game"))
            .await
            .unwrap();
        assert!(
            headless
                .candidates
                .iter()
                .all(|candidate| candidate.relation == ProgressRelation::NoLocalPosition)
        );
    }

    #[tokio::test]
    async fn rejects_conflicting_local_snapshot_identity() {
        let root = temp_dir::TempDir::new().unwrap();
        let mut game = GameManifest::new("game");
        game.upsert_live(node("root", None)).unwrap();
        game.upsert_live(node("same", Some("root"))).unwrap();
        game.set_head("deck".into(), "same".into());
        let mut local = GameSnapshots::new("Game");
        local.backups = vec![snapshot("same", None)];

        assert!(matches!(
            inspector(game, root.path())
                .await
                .review("game", &local)
                .await,
            Err(ConflictReviewError::SnapshotIdentityConflict(id)) if id == "same"
        ));
    }
}
