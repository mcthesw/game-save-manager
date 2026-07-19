use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::ArchiveIntegrity;
use crate::backup::{ArchiveFormat, CreatedBy};
use crate::device::DeviceId;

pub const CLOUD_MANIFEST_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudManifest {
    pub schema_version: u32,
    pub revision: u64,
    pub games: BTreeMap<String, GameManifest>,
}

impl Default for CloudManifest {
    fn default() -> Self {
        Self {
            schema_version: CLOUD_MANIFEST_SCHEMA_VERSION,
            revision: 0,
            games: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameManifest {
    pub game_id: String,
    pub snapshots: BTreeMap<String, SnapshotNode>,
    pub device_heads: BTreeMap<DeviceId, String>,
    pub local_archives: BTreeMap<DeviceId, BTreeSet<String>>,
}

impl GameManifest {
    pub fn new(game_id: impl Into<String>) -> Self {
        Self {
            game_id: game_id.into(),
            snapshots: BTreeMap::new(),
            device_heads: BTreeMap::new(),
            local_archives: BTreeMap::new(),
        }
    }

    pub fn upsert_live(&mut self, node: SnapshotNode) -> Result<(), ManifestError> {
        if !node.state.is_live() {
            return Err(ManifestError::ExpectedLive(node.snapshot_id));
        }
        if self
            .snapshots
            .get(&node.snapshot_id)
            .is_some_and(|existing| !existing.state.is_live())
        {
            return Err(ManifestError::TombstoneResurrection(node.snapshot_id));
        }
        self.snapshots.insert(node.snapshot_id.clone(), node);
        Ok(())
    }

    pub fn set_head(&mut self, device_id: DeviceId, snapshot_id: String) {
        self.device_heads.insert(device_id, snapshot_id);
    }

    pub fn report_local_archive(
        &mut self,
        device_id: DeviceId,
        snapshot_id: String,
        present: bool,
    ) {
        let report = self.local_archives.entry(device_id).or_default();
        let accepts_presence = self
            .snapshots
            .get(&snapshot_id)
            .is_some_and(|node| node.state.is_live());
        if present && accepts_presence {
            report.insert(snapshot_id);
        } else {
            report.remove(&snapshot_id);
        }
    }

    pub fn begin_deletion(
        &mut self,
        snapshot_id: &str,
        acting_device: &str,
        kind: DeletionKind,
    ) -> Result<bool, ManifestError> {
        let node = self
            .snapshots
            .get_mut(snapshot_id)
            .ok_or_else(|| ManifestError::MissingSnapshot(snapshot_id.to_string()))?;
        match &node.state {
            SnapshotState::FinalTombstone { kind: existing } if existing == &kind => {
                return Ok(false);
            }
            SnapshotState::PendingTombstone(existing)
                if existing.kind == kind && existing.acting_device == acting_device =>
            {
                return Ok(false);
            }
            SnapshotState::Live(_) => {}
            _ => return Err(ManifestError::DeletionConflict(snapshot_id.to_string())),
        }
        node.state = SnapshotState::PendingTombstone(PendingTombstone {
            kind,
            acting_device: acting_device.to_string(),
            acting_local_removed: false,
            cloud_archive_absent: false,
        });
        self.device_heads.retain(|_, head| head != snapshot_id);
        for report in self.local_archives.values_mut() {
            report.remove(snapshot_id);
        }
        Ok(true)
    }

    pub fn mark_acting_local_removed(
        &mut self,
        snapshot_id: &str,
        acting_device: &str,
    ) -> Result<(), ManifestError> {
        {
            let pending = self.pending_mut(snapshot_id, acting_device)?;
            pending.acting_local_removed = true;
        }
        self.report_local_archive(acting_device.to_string(), snapshot_id.to_string(), false);
        Ok(())
    }

    pub fn mark_cloud_absent(
        &mut self,
        snapshot_id: &str,
        acting_device: &str,
    ) -> Result<(), ManifestError> {
        let node = self
            .snapshots
            .get_mut(snapshot_id)
            .ok_or_else(|| ManifestError::MissingSnapshot(snapshot_id.to_string()))?;
        let SnapshotState::PendingTombstone(pending) = &mut node.state else {
            return if matches!(node.state, SnapshotState::FinalTombstone { .. }) {
                Ok(())
            } else {
                Err(ManifestError::DeletionConflict(snapshot_id.to_string()))
            };
        };
        if pending.acting_device != acting_device {
            return Err(ManifestError::DeletionConflict(snapshot_id.to_string()));
        }
        pending.cloud_archive_absent = true;
        if pending.acting_local_removed {
            node.state = SnapshotState::FinalTombstone {
                kind: pending.kind.clone(),
            };
        }
        Ok(())
    }

    fn pending_mut(
        &mut self,
        snapshot_id: &str,
        acting_device: &str,
    ) -> Result<&mut PendingTombstone, ManifestError> {
        let node = self
            .snapshots
            .get_mut(snapshot_id)
            .ok_or_else(|| ManifestError::MissingSnapshot(snapshot_id.to_string()))?;
        if matches!(node.state, SnapshotState::FinalTombstone { .. }) {
            return Err(ManifestError::DeletionAlreadyFinal(snapshot_id.to_string()));
        }
        let SnapshotState::PendingTombstone(pending) = &mut node.state else {
            return Err(ManifestError::DeletionConflict(snapshot_id.to_string()));
        };
        if pending.acting_device != acting_device {
            return Err(ManifestError::DeletionConflict(snapshot_id.to_string()));
        }
        Ok(pending)
    }

    pub fn is_final_tombstone(&self, snapshot_id: &str) -> bool {
        self.snapshots
            .get(snapshot_id)
            .is_some_and(|node| matches!(node.state, SnapshotState::FinalTombstone { .. }))
    }

    /// Return whether `ancestor` is the same node as, or an ancestor of, `descendant`.
    ///
    /// Time is O(V) and safety space is O(V) for V Snapshot nodes.
    pub fn is_ancestor_or_equal(
        &self,
        ancestor: &str,
        descendant: &str,
    ) -> Result<bool, ManifestError> {
        if !self.snapshots.contains_key(ancestor) {
            return Err(ManifestError::MissingSnapshot(ancestor.to_string()));
        }
        let mut cursor = descendant;
        let mut visited = HashSet::new();
        loop {
            if cursor == ancestor {
                return Ok(true);
            }
            if !visited.insert(cursor) {
                return Err(ManifestError::ParentCycle(cursor.to_string()));
            }
            let node = self
                .snapshots
                .get(cursor)
                .ok_or_else(|| ManifestError::MissingSnapshot(cursor.to_string()))?;
            let Some(parent) = node.parent.as_deref() else {
                return Ok(false);
            };
            cursor = parent;
        }
    }

    /// Return distinct Device Head values that are not ancestors of another Head.
    ///
    /// For H Heads and V nodes this intentionally simple implementation is
    /// O(H²V) time and O(H + V) additional space.
    pub fn maximal_heads(&self) -> Result<BTreeSet<String>, ManifestError> {
        self.validate()?;
        let heads = self.device_heads.values().cloned().collect::<BTreeSet<_>>();
        let mut maximal = BTreeSet::new();
        for candidate in &heads {
            let mut dominated = false;
            for other in &heads {
                if candidate != other && self.is_ancestor_or_equal(candidate, other)? {
                    dominated = true;
                    break;
                }
            }
            if !dominated {
                maximal.insert(candidate.clone());
            }
        }
        Ok(maximal)
    }

    fn validate_parent_chain(&self, start: &str) -> Result<(), ManifestError> {
        let mut cursor = start;
        let mut visited = HashSet::new();
        loop {
            if !visited.insert(cursor) {
                return Err(ManifestError::ParentCycle(cursor.to_string()));
            }
            let node = self
                .snapshots
                .get(cursor)
                .ok_or_else(|| ManifestError::MissingSnapshot(cursor.to_string()))?;
            let Some(parent) = node.parent.as_deref() else {
                return Ok(());
            };
            cursor = parent;
        }
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        for (snapshot_id, node) in &self.snapshots {
            if snapshot_id != &node.snapshot_id {
                return Err(ManifestError::SnapshotIdentityMismatch {
                    key: snapshot_id.clone(),
                    embedded: node.snapshot_id.clone(),
                });
            }
            if let Some(parent) = &node.parent
                && !self.snapshots.contains_key(parent)
            {
                return Err(ManifestError::MissingParent {
                    snapshot: snapshot_id.clone(),
                    parent: parent.clone(),
                });
            }
            if let SnapshotState::Live(live) = &node.state {
                if live.cloud_archive_verified && live.integrity.is_none() {
                    return Err(ManifestError::InvalidIntegrity(snapshot_id.clone()));
                }
                if let Some(integrity) = &live.integrity
                    && (integrity.xxh3_64.len() != 16
                        || !integrity
                            .xxh3_64
                            .bytes()
                            .all(|byte| byte.is_ascii_hexdigit()))
                {
                    return Err(ManifestError::InvalidIntegrity(snapshot_id.clone()));
                }
            }
        }
        for snapshot_id in self.snapshots.keys() {
            self.validate_parent_chain(snapshot_id)?;
        }
        for (device_id, head) in &self.device_heads {
            let node = self
                .snapshots
                .get(head)
                .ok_or_else(|| ManifestError::InvalidHead {
                    device: device_id.clone(),
                    snapshot: head.clone(),
                })?;
            if !node.state.is_live() {
                return Err(ManifestError::InvalidHead {
                    device: device_id.clone(),
                    snapshot: head.clone(),
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotNode {
    pub snapshot_id: String,
    pub parent: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub archive_format: ArchiveFormat,
    pub state: SnapshotState,
}

impl SnapshotNode {
    pub fn live(
        snapshot_id: impl Into<String>,
        parent: Option<String>,
        integrity: ArchiveIntegrity,
        created_by: CreatedBy,
    ) -> Self {
        Self {
            snapshot_id: snapshot_id.into(),
            parent,
            description: String::new(),
            archive_format: ArchiveFormat::default(),
            state: SnapshotState::Live(LiveSnapshot {
                integrity: Some(integrity),
                cloud_archive_verified: false,
                created_by,
                retention_protected: false,
            }),
        }
    }

    pub fn unavailable(
        snapshot_id: impl Into<String>,
        parent: Option<String>,
        created_by: CreatedBy,
    ) -> Self {
        Self {
            snapshot_id: snapshot_id.into(),
            parent,
            description: String::new(),
            archive_format: ArchiveFormat::default(),
            state: SnapshotState::Live(LiveSnapshot {
                integrity: None,
                cloud_archive_verified: false,
                created_by,
                retention_protected: false,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SnapshotState {
    Live(LiveSnapshot),
    PendingTombstone(PendingTombstone),
    FinalTombstone { kind: DeletionKind },
}

impl SnapshotState {
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Live(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveSnapshot {
    #[serde(default)]
    pub integrity: Option<ArchiveIntegrity>,
    pub cloud_archive_verified: bool,
    pub created_by: CreatedBy,
    pub retention_protected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingTombstone {
    pub kind: DeletionKind,
    pub acting_device: DeviceId,
    pub acting_local_removed: bool,
    pub cloud_archive_absent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeletionKind {
    User,
    Retention,
    GameDeletion,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("Unsupported Cloud Manifest schema {found}; expected {expected}")]
    UnsupportedSchema { expected: u32, found: u32 },
    #[error("Game identity mismatch: key {key}, embedded {embedded}")]
    GameIdentityMismatch { key: String, embedded: String },
    #[error("Snapshot identity mismatch: key {key}, embedded {embedded}")]
    SnapshotIdentityMismatch { key: String, embedded: String },
    #[error("Snapshot {snapshot} references missing parent {parent}")]
    MissingParent { snapshot: String, parent: String },
    #[error("Snapshot parent cycle contains {0}")]
    ParentCycle(String),
    #[error("Device {device} Head references missing or non-live Snapshot {snapshot}")]
    InvalidHead { device: DeviceId, snapshot: String },
    #[error("Snapshot does not exist: {0}")]
    MissingSnapshot(String),
    #[error("Expected a live Snapshot: {0}")]
    ExpectedLive(String),
    #[error("Snapshot has invalid XXH3-64 integrity: {0}")]
    InvalidIntegrity(String),
    #[error("Tombstoned Snapshot cannot be resurrected: {0}")]
    TombstoneResurrection(String),
    #[error("Snapshot deletion conflicts with existing Tombstone state: {0}")]
    DeletionConflict(String),
    #[error("Snapshot deletion is already Final: {0}")]
    DeletionAlreadyFinal(String),
}

impl CloudManifest {
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != CLOUD_MANIFEST_SCHEMA_VERSION {
            return Err(ManifestError::UnsupportedSchema {
                expected: CLOUD_MANIFEST_SCHEMA_VERSION,
                found: self.schema_version,
            });
        }
        for (game_id, game) in &self.games {
            if game_id != &game.game_id {
                return Err(ManifestError::GameIdentityMismatch {
                    key: game_id.clone(),
                    embedded: game.game_id.clone(),
                });
            }
            game.validate()?;
        }
        Ok(())
    }

    pub fn game_mut(&mut self, game_id: &str) -> &mut GameManifest {
        self.games
            .entry(game_id.to_string())
            .or_insert_with(|| GameManifest::new(game_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn integrity() -> ArchiveIntegrity {
        ArchiveIntegrity {
            size: 10,
            xxh3_64: "0123456789abcdef".into(),
        }
    }

    fn live(id: &str, parent: Option<&str>) -> SnapshotNode {
        SnapshotNode::live(
            id,
            parent.map(str::to_string),
            integrity(),
            CreatedBy::Manual,
        )
    }

    #[test]
    fn parallel_heads_return_only_maximal_lineages() {
        let mut game = GameManifest::new("game");
        for node in [
            live("root", None),
            live("a", Some("root")),
            live("a2", Some("a")),
            live("b", Some("root")),
        ] {
            game.upsert_live(node).unwrap();
        }
        game.set_head("pc".into(), "a".into());
        game.set_head("deck".into(), "a2".into());
        game.set_head("laptop".into(), "b".into());

        assert_eq!(
            game.maximal_heads().unwrap(),
            BTreeSet::from(["a2".to_string(), "b".to_string()])
        );
        assert!(matches!(
            game.is_ancestor_or_equal("missing", "root"),
            Err(ManifestError::MissingSnapshot(_))
        ));
    }

    #[test]
    fn malformed_graphs_fail_closed() {
        let mut missing = GameManifest::new("game");
        missing.upsert_live(live("child", Some("missing"))).unwrap();
        assert!(matches!(
            missing.validate(),
            Err(ManifestError::MissingParent { .. })
        ));

        let mut cycle = GameManifest::new("game");
        cycle.upsert_live(live("a", Some("b"))).unwrap();
        cycle.upsert_live(live("b", Some("a"))).unwrap();
        assert!(matches!(
            cycle.validate(),
            Err(ManifestError::ParentCycle(_))
        ));

        let mut invalid_head = GameManifest::new("game");
        invalid_head.set_head("pc".into(), "missing".into());
        assert!(matches!(
            invalid_head.validate(),
            Err(ManifestError::InvalidHead { .. })
        ));
    }

    #[test]
    fn tombstone_dominates_stale_live_upsert() {
        let mut game = GameManifest::new("game");
        game.upsert_live(live("snapshot", None)).unwrap();
        game.begin_deletion("snapshot", "pc", DeletionKind::User)
            .unwrap();
        game.report_local_archive("deck".into(), "snapshot".into(), true);
        assert!(!game.local_archives["deck"].contains("snapshot"));

        assert_eq!(
            game.upsert_live(live("snapshot", None)),
            Err(ManifestError::TombstoneResurrection("snapshot".to_string()))
        );

        game.mark_acting_local_removed("snapshot", "pc").unwrap();
        game.mark_cloud_absent("snapshot", "pc").unwrap();
        assert!(game.is_final_tombstone("snapshot"));
        assert_eq!(
            game.upsert_live(live("snapshot", None)),
            Err(ManifestError::TombstoneResurrection("snapshot".to_string()))
        );
    }

    #[test]
    fn unsupported_schema_fails_closed() {
        let mut manifest = CloudManifest::default();
        manifest.schema_version += 1;

        assert!(matches!(
            manifest.validate(),
            Err(ManifestError::UnsupportedSchema { .. })
        ));

        let mut invalid_integrity = CloudManifest::default();
        invalid_integrity
            .game_mut("game")
            .upsert_live(SnapshotNode::live(
                "snapshot",
                None,
                ArchiveIntegrity {
                    size: 1,
                    xxh3_64: "not-a-hash".into(),
                },
                CreatedBy::Manual,
            ))
            .unwrap();
        assert!(matches!(
            invalid_integrity.validate(),
            Err(ManifestError::InvalidIntegrity(_))
        ));
    }

    #[test]
    fn deterministic_manifest_round_trip() {
        let mut manifest = CloudManifest::default();
        let game = manifest.game_mut("game");
        game.upsert_live(live("root", None)).unwrap();
        game.report_local_archive("pc".into(), "root".into(), true);
        manifest.validate().unwrap();

        let first = serde_json::to_vec_pretty(&manifest).unwrap();
        let decoded: CloudManifest = serde_json::from_slice(&first).unwrap();
        let second = serde_json::to_vec_pretty(&decoded).unwrap();

        assert_eq!(first, second);
    }
}
