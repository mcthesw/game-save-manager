use std::collections::{BTreeMap, BTreeSet, VecDeque};

use thiserror::Error;

use super::{GameManifest, ManifestError, SnapshotState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotRetentionPlan {
    /// Snapshot identity is the stable set key. The set intentionally does not
    /// imply a recency order between incomparable Branches.
    pub candidates: BTreeSet<String>,
}

pub struct SnapshotRetentionPlanner;

impl SnapshotRetentionPlanner {
    /// Select globally deletable automatic Snapshots while preserving the
    /// newest `limit` eligible nodes on every live Branch.
    ///
    /// The input must be a validated parent forest. A parent/child index and
    /// memoized post-order live-descendant pass find maximal live endpoints in
    /// O(V + E). A second depth-first traversal maintains one bounded lineage
    /// window, so selection is O(V + E) time and O(V + limit) space. Missing
    /// parents, cycles, invalid Heads, or conflicting identities fail closed
    /// through `GameManifest::validate` before any candidate is returned.
    pub fn plan(
        game: &GameManifest,
        limit: usize,
    ) -> Result<SnapshotRetentionPlan, SnapshotRetentionPlannerError> {
        game.validate()?;
        let children = child_index(game);
        let endpoints = maximal_live_endpoints(game, &children);
        let active_heads = game.device_heads.values().cloned().collect::<BTreeSet<_>>();
        let eligible = game
            .snapshots
            .iter()
            .filter_map(|(snapshot_id, node)| {
                let SnapshotState::Live(live) = &node.state else {
                    return None;
                };
                (live.created_by.is_automatic_backup()
                    && !live.retention_protected
                    && !active_heads.contains(snapshot_id))
                .then(|| snapshot_id.clone())
            })
            .collect::<BTreeSet<_>>();
        let retained = retained_on_every_branch(game, &children, &endpoints, &eligible, limit);
        Ok(SnapshotRetentionPlan {
            candidates: eligible.difference(&retained).cloned().collect(),
        })
    }
}

fn child_index(game: &GameManifest) -> BTreeMap<String, Vec<String>> {
    let mut children = game
        .snapshots
        .keys()
        .map(|snapshot_id| (snapshot_id.clone(), Vec::new()))
        .collect::<BTreeMap<_, _>>();
    for node in game.snapshots.values() {
        if let Some(parent) = &node.parent {
            children
                .get_mut(parent)
                .expect("validated parent exists")
                .push(node.snapshot_id.clone());
        }
    }
    children
}

/// A live endpoint is a live node with no live descendant. Tombstones remain
/// traversable, so a live node can be dominated by a live grandchild through a
/// Tombstone. The memoized post-order map visits every edge exactly once.
fn maximal_live_endpoints(
    game: &GameManifest,
    children: &BTreeMap<String, Vec<String>>,
) -> BTreeSet<String> {
    let roots = game
        .snapshots
        .values()
        .filter(|node| node.parent.is_none())
        .map(|node| node.snapshot_id.clone())
        .collect::<Vec<_>>();
    let mut subtree_has_live = BTreeMap::<String, bool>::new();
    let mut endpoints = BTreeSet::new();
    let mut stack = roots
        .into_iter()
        .rev()
        .map(|snapshot_id| (snapshot_id, false))
        .collect::<Vec<_>>();
    while let Some((snapshot_id, expanded)) = stack.pop() {
        if !expanded {
            stack.push((snapshot_id.clone(), true));
            for child in children[&snapshot_id].iter().rev() {
                stack.push((child.clone(), false));
            }
            continue;
        }
        let child_has_live = children[&snapshot_id]
            .iter()
            .any(|child| subtree_has_live[child]);
        let is_live = game.snapshots[&snapshot_id].state.is_live();
        if is_live && !child_has_live {
            endpoints.insert(snapshot_id.clone());
        }
        subtree_has_live.insert(snapshot_id, is_live || child_has_live);
    }
    endpoints
}

enum LineageVisit {
    Enter(String),
    Exit {
        snapshot_id: String,
        evicted_ancestor: Option<String>,
    },
}

/// Walk each forest root once. Enter/Exit events backtrack one shared lineage
/// deque instead of copying complete ancestor paths for every Branch.
fn retained_on_every_branch(
    game: &GameManifest,
    children: &BTreeMap<String, Vec<String>>,
    endpoints: &BTreeSet<String>,
    eligible: &BTreeSet<String>,
    limit: usize,
) -> BTreeSet<String> {
    let roots = game
        .snapshots
        .values()
        .filter(|node| node.parent.is_none())
        .map(|node| node.snapshot_id.clone())
        .collect::<Vec<_>>();
    let mut retained = BTreeSet::new();
    let mut lineage = VecDeque::<String>::new();
    let mut stack = roots
        .into_iter()
        .rev()
        .map(LineageVisit::Enter)
        .collect::<Vec<_>>();
    while let Some(visit) = stack.pop() {
        match visit {
            LineageVisit::Enter(snapshot_id) => {
                let evicted = if eligible.contains(&snapshot_id) {
                    lineage.push_back(snapshot_id.clone());
                    (lineage.len() > limit).then(|| lineage.pop_front().expect("non-empty lineage"))
                } else {
                    None
                };
                let evicted_ancestor = evicted.filter(|evicted_id| evicted_id != &snapshot_id);
                if endpoints.contains(&snapshot_id) {
                    retained.extend(lineage.iter().cloned());
                }
                stack.push(LineageVisit::Exit {
                    snapshot_id: snapshot_id.clone(),
                    evicted_ancestor,
                });
                for child in children[&snapshot_id].iter().rev() {
                    stack.push(LineageVisit::Enter(child.clone()));
                }
            }
            LineageVisit::Exit {
                snapshot_id,
                evicted_ancestor,
            } => {
                if lineage.back() == Some(&snapshot_id) {
                    lineage.pop_back();
                }
                if let Some(ancestor) = evicted_ancestor {
                    lineage.push_front(ancestor);
                }
            }
        }
    }
    retained
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SnapshotRetentionPlannerError {
    #[error(transparent)]
    Manifest(#[from] ManifestError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::CreatedBy;
    use crate::cloud_sync::v2::{ArchiveIntegrity, DeletionKind, SnapshotNode, SnapshotState};

    fn node(id: &str, parent: Option<&str>, created_by: CreatedBy) -> SnapshotNode {
        SnapshotNode::live(
            id,
            parent.map(str::to_string),
            ArchiveIntegrity {
                size: 1,
                xxh3_64: "0000000000000000".into(),
            },
            created_by,
        )
    }

    fn insert(game: &mut GameManifest, node: SnapshotNode) {
        game.upsert_live(node).unwrap();
    }

    #[test]
    fn linear_branch_keeps_newest_eligible_nodes_without_counting_the_head() {
        let mut game = GameManifest::new("game");
        insert(&mut game, node("a", None, CreatedBy::Timer));
        insert(&mut game, node("b", Some("a"), CreatedBy::Timer));
        insert(&mut game, node("c", Some("b"), CreatedBy::Timer));
        insert(&mut game, node("d", Some("c"), CreatedBy::Timer));
        insert(&mut game, node("e", Some("d"), CreatedBy::Timer));
        game.set_head("pc".into(), "e".into());

        let plan = SnapshotRetentionPlanner::plan(&game, 2).unwrap();

        assert_eq!(plan.candidates, BTreeSet::from(["a".into(), "b".into()]));
    }

    #[test]
    fn shared_ancestor_stays_when_any_branch_still_selects_it() {
        let mut game = GameManifest::new("game");
        insert(&mut game, node("root", None, CreatedBy::Timer));
        insert(&mut game, node("short", Some("root"), CreatedBy::Manual));
        insert(&mut game, node("long-1", Some("root"), CreatedBy::Timer));
        insert(&mut game, node("long-2", Some("long-1"), CreatedBy::Timer));

        let plan = SnapshotRetentionPlanner::plan(&game, 1).unwrap();

        assert_eq!(plan.candidates, BTreeSet::from(["long-1".into()]));
    }

    #[test]
    fn manual_protected_and_head_snapshots_are_never_candidates() {
        let mut game = GameManifest::new("game");
        insert(&mut game, node("manual", None, CreatedBy::Manual));
        let mut protected = node("protected", Some("manual"), CreatedBy::Timer);
        let SnapshotState::Live(live) = &mut protected.state else {
            unreachable!()
        };
        live.retention_protected = true;
        insert(&mut game, protected);
        insert(
            &mut game,
            node("automatic", Some("protected"), CreatedBy::Timer),
        );
        insert(&mut game, node("head", Some("automatic"), CreatedBy::Timer));
        game.set_head("deck".into(), "head".into());

        let plan = SnapshotRetentionPlanner::plan(&game, 0).unwrap();

        assert_eq!(plan.candidates, BTreeSet::from(["automatic".into()]));
    }

    #[test]
    fn lineage_walk_crosses_tombstones() {
        let mut game = GameManifest::new("game");
        insert(&mut game, node("old", None, CreatedBy::Timer));
        let mut deleted = node("deleted", Some("old"), CreatedBy::Timer);
        deleted.state = SnapshotState::FinalTombstone {
            kind: DeletionKind::User,
        };
        game.snapshots.insert("deleted".into(), deleted);
        insert(
            &mut game,
            node("current", Some("deleted"), CreatedBy::Timer),
        );
        game.set_head("pc".into(), "current".into());

        let plan = SnapshotRetentionPlanner::plan(&game, 0).unwrap();

        assert_eq!(plan.candidates, BTreeSet::from(["old".into()]));
    }

    #[test]
    fn malformed_graph_fails_before_returning_candidates() {
        let mut game = GameManifest::new("game");
        insert(&mut game, node("snapshot", None, CreatedBy::Timer));
        game.snapshots.get_mut("snapshot").unwrap().parent = Some("missing".into());

        assert!(matches!(
            SnapshotRetentionPlanner::plan(&game, 1),
            Err(SnapshotRetentionPlannerError::Manifest(
                ManifestError::MissingParent { .. }
            ))
        ));
    }
}
