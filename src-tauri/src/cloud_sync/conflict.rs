use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::backup::GameSnapshots;

/// Describes the relationship between local and remote snapshot trees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SyncRelation {
    /// Both sides point to the same HEAD — nothing to do.
    InSync,
    /// Local HEAD is an ancestor of remote HEAD → safe to fast-forward pull.
    LocalBehind,
    /// Remote HEAD is an ancestor of local HEAD → safe to push.
    LocalAhead,
    /// Both HEADs exist on the same tree but diverged — not a true conflict
    /// because each device is on a different branch.
    Diverged,
    /// True conflict: competing updates on the same HEAD that cannot be
    /// auto-resolved. User must choose A / B / fork.
    Conflict,
    /// Cannot determine relationship (e.g., one side has no HEAD).
    Unknown,
}

/// What the user chose to do when a conflict is detected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    /// Keep local state, overwrite remote.
    KeepLocal,
    /// Accept remote state, overwrite local.
    AcceptRemote,
    /// Keep both branches — create a fork point.
    Fork,
    /// User cancelled — do nothing for now.
    Cancelled,
}

/// Compare local and remote snapshot metadata to determine sync relationship.
///
/// This function uses snapshot-tree semantics:
/// - Two HEADs on **different branches** (neither is ancestor of the other,
///   but both share a common ancestor) → `Diverged` (not conflict).
/// - Same HEAD → `InSync`.
/// - One HEAD is reachable from the other → `LocalBehind` / `LocalAhead`.
/// - Otherwise → `Conflict`.
pub fn determine_sync_relation(local: &GameSnapshots, remote: &GameSnapshots) -> SyncRelation {
    let local_head = match &local.head {
        Some(h) => h.clone(),
        None => {
            return if remote.head.is_some() {
                SyncRelation::LocalBehind
            } else {
                SyncRelation::InSync
            };
        }
    };

    let remote_head = match &remote.head {
        Some(h) => h.clone(),
        None => return SyncRelation::LocalAhead,
    };

    // Same HEAD → in sync.
    if local_head == remote_head {
        // But check if local has newer snapshots not on remote.
        let local_dates: HashSet<_> = local.backups.iter().map(|s| &s.date).collect();
        let remote_dates: HashSet<_> = remote.backups.iter().map(|s| &s.date).collect();
        let local_only: Vec<_> = local_dates.difference(&remote_dates).collect();
        let remote_only: Vec<_> = remote_dates.difference(&local_dates).collect();

        if local_only.is_empty() && remote_only.is_empty() {
            return SyncRelation::InSync;
        }
        if !local_only.is_empty() && remote_only.is_empty() {
            return SyncRelation::LocalAhead;
        }
        if local_only.is_empty() && !remote_only.is_empty() {
            return SyncRelation::LocalBehind;
        }
        // Both have unique snapshots but same HEAD — diverged, not conflict.
        return SyncRelation::Diverged;
    }

    // Check ancestry: is remote_head reachable from local snapshots?
    let remote_head_in_local = is_ancestor(&local_head, &remote_head, &local.backups);
    let local_head_in_remote = is_ancestor(&remote_head, &local_head, &remote.backups);

    match (remote_head_in_local, local_head_in_remote) {
        // Remote HEAD is an ancestor of local HEAD → local is ahead.
        (true, _) => SyncRelation::LocalAhead,
        // Local HEAD is an ancestor of remote HEAD → local is behind.
        (_, true) => SyncRelation::LocalBehind,
        // Neither is ancestor of the other → check if they share a common root.
        _ => {
            // If they share any common snapshot, they diverged from a common point.
            let local_dates: HashSet<_> = local.backups.iter().map(|s| &s.date).collect();
            let has_common = remote.backups.iter().any(|s| local_dates.contains(&s.date));
            if has_common {
                SyncRelation::Diverged
            } else {
                SyncRelation::Conflict
            }
        }
    }
}

/// Walk the parent chain from `from` and check if `target` is reachable.
fn is_ancestor(from: &str, target: &str, snapshots: &[crate::backup::Snapshot]) -> bool {
    let mut current = from.to_string();
    let mut visited = HashSet::new();
    loop {
        if current == target {
            return true;
        }
        if !visited.insert(current.clone()) {
            return false; // cycle guard
        }
        match snapshots.iter().find(|s| s.date == current) {
            Some(s) => match &s.parent {
                Some(p) => current = p.clone(),
                None => return false,
            },
            None => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::{GameSnapshots, Snapshot};

    fn snap(date: &str, parent: Option<&str>) -> Snapshot {
        Snapshot {
            date: date.to_string(),
            describe: String::new(),
            path: String::new(),
            size: 0,
            parent: parent.map(|s| s.to_string()),
            archive_hash: None,
            device_id: None,
        }
    }

    fn gs(name: &str, backups: Vec<Snapshot>, head: Option<&str>) -> GameSnapshots {
        GameSnapshots {
            name: name.to_string(),
            backups,
            head: head.map(|s| s.to_string()),
            sync_version: 0,
            last_sync_device: None,
            last_sync_timestamp: None,
        }
    }

    #[test]
    fn both_empty_is_in_sync() {
        let local = gs("g", vec![], None);
        let remote = gs("g", vec![], None);
        assert_eq!(
            determine_sync_relation(&local, &remote),
            SyncRelation::InSync
        );
    }

    #[test]
    fn local_has_no_head_remote_has() {
        let local = gs("g", vec![], None);
        let remote = gs("g", vec![snap("a", None)], Some("a"));
        assert_eq!(
            determine_sync_relation(&local, &remote),
            SyncRelation::LocalBehind
        );
    }

    #[test]
    fn local_has_head_remote_empty() {
        let local = gs("g", vec![snap("a", None)], Some("a"));
        let remote = gs("g", vec![], None);
        assert_eq!(
            determine_sync_relation(&local, &remote),
            SyncRelation::LocalAhead
        );
    }

    #[test]
    fn same_head_same_snapshots_is_in_sync() {
        let snaps = vec![snap("a", None), snap("b", Some("a"))];
        let local = gs("g", snaps.clone(), Some("b"));
        let remote = gs("g", snaps, Some("b"));
        assert_eq!(
            determine_sync_relation(&local, &remote),
            SyncRelation::InSync
        );
    }

    #[test]
    fn local_ahead_linear_chain() {
        // a → b → c (local HEAD=c, remote HEAD=b)
        let snaps = vec![snap("a", None), snap("b", Some("a")), snap("c", Some("b"))];
        let local = gs("g", snaps.clone(), Some("c"));
        let remote = gs("g", vec![snap("a", None), snap("b", Some("a"))], Some("b"));
        assert_eq!(
            determine_sync_relation(&local, &remote),
            SyncRelation::LocalAhead
        );
    }

    #[test]
    fn local_behind_linear_chain() {
        let local = gs("g", vec![snap("a", None), snap("b", Some("a"))], Some("b"));
        let remote = gs(
            "g",
            vec![snap("a", None), snap("b", Some("a")), snap("c", Some("b"))],
            Some("c"),
        );
        assert_eq!(
            determine_sync_relation(&local, &remote),
            SyncRelation::LocalBehind
        );
    }

    #[test]
    fn diverged_different_branches_from_common_ancestor() {
        // Both share snapshot "a", local goes a→b, remote goes a→c
        let local = gs("g", vec![snap("a", None), snap("b", Some("a"))], Some("b"));
        let remote = gs("g", vec![snap("a", None), snap("c", Some("a"))], Some("c"));
        assert_eq!(
            determine_sync_relation(&local, &remote),
            SyncRelation::Diverged
        );
    }

    #[test]
    fn conflict_no_common_snapshots() {
        let local = gs("g", vec![snap("x", None)], Some("x"));
        let remote = gs("g", vec![snap("y", None)], Some("y"));
        assert_eq!(
            determine_sync_relation(&local, &remote),
            SyncRelation::Conflict
        );
    }
}
