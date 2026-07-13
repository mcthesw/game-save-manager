use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::backup::GameSnapshots;
use crate::device::{DeviceId, get_current_device_id};

/// Describes the relationship between local and remote snapshot trees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SyncRelation {
    /// Both sides already describe the same snapshot graph and per-device heads.
    InSync,
    /// Local metadata can safely advance the current device state on remote.
    CurrentDeviceAhead,
    /// Remote metadata can safely advance the current device state locally.
    CurrentDeviceBehind,
    /// Both sides share history, but each side contains additional branches or
    /// device heads that should coexist.
    SharedTreeDiverged,
    /// Both sides have disjoint histories; they are parallel branches rather
    /// than a user-facing conflict.
    ParallelBranches,
    /// Metadata is internally inconsistent (for example, the same device points
    /// to incompatible branches on local and remote).
    IncompatibleState,
}

/// What the user chose to do when a conflict is detected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    KeepLocal,
    AcceptRemote,
    /// Preserve both local and remote branches without merging.
    ///
    /// TODO: implement branch selection and upload semantics for this git-like fork workflow.
    Fork,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceHeadRelation {
    Same,
    LocalAhead,
    LocalBehind,
    LocalMissing,
    RemoteMissing,
    Diverged,
}

pub fn determine_sync_relation(local: &GameSnapshots, remote: &GameSnapshots) -> SyncRelation {
    let Ok(parent_map) = build_parent_map(local, remote) else {
        return SyncRelation::IncompatibleState;
    };

    if has_incompatible_same_device_heads(local, remote, &parent_map) {
        return SyncRelation::IncompatibleState;
    }

    let local_dates: HashSet<_> = local
        .backups
        .iter()
        .map(|snapshot| snapshot.date.as_str())
        .collect();
    let remote_dates: HashSet<_> = remote
        .backups
        .iter()
        .map(|snapshot| snapshot.date.as_str())
        .collect();
    let local_only = local_dates.difference(&remote_dates).count();
    let remote_only = remote_dates.difference(&local_dates).count();
    let has_common_snapshot = !local_dates.is_disjoint(&remote_dates);

    let current_device_relation =
        compare_device_head(get_current_device_id(), local, remote, &parent_map);

    if local_only == 0 && remote_only == 0 {
        if local.device_heads == remote.device_heads {
            return SyncRelation::InSync;
        }

        return match current_device_relation {
            DeviceHeadRelation::LocalAhead | DeviceHeadRelation::RemoteMissing => {
                SyncRelation::CurrentDeviceAhead
            }
            DeviceHeadRelation::LocalBehind | DeviceHeadRelation::LocalMissing => {
                SyncRelation::CurrentDeviceBehind
            }
            DeviceHeadRelation::Same => SyncRelation::SharedTreeDiverged,
            DeviceHeadRelation::Diverged => SyncRelation::IncompatibleState,
        };
    }

    if remote_only == 0 {
        return SyncRelation::CurrentDeviceAhead;
    }

    if local_only == 0 {
        return SyncRelation::CurrentDeviceBehind;
    }

    if has_common_snapshot {
        SyncRelation::SharedTreeDiverged
    } else {
        SyncRelation::ParallelBranches
    }
}

fn build_parent_map(
    local: &GameSnapshots,
    remote: &GameSnapshots,
) -> Result<HashMap<String, Option<String>>, ()> {
    let mut parent_map = HashMap::new();

    for snapshot in local.backups.iter().chain(remote.backups.iter()) {
        match parent_map.get(&snapshot.date) {
            Some(existing_parent) if existing_parent != &snapshot.parent => return Err(()),
            Some(_) => {}
            None => {
                parent_map.insert(snapshot.date.clone(), snapshot.parent.clone());
            }
        }
    }

    Ok(parent_map)
}

fn has_incompatible_same_device_heads(
    local: &GameSnapshots,
    remote: &GameSnapshots,
    parent_map: &HashMap<String, Option<String>>,
) -> bool {
    let shared_devices: HashSet<&DeviceId> = local
        .device_heads
        .keys()
        .filter(|device_id| remote.device_heads.contains_key(*device_id))
        .collect();

    shared_devices.into_iter().any(|device_id| {
        compare_device_head(device_id, local, remote, parent_map) == DeviceHeadRelation::Diverged
    })
}

fn compare_device_head(
    device_id: &DeviceId,
    local: &GameSnapshots,
    remote: &GameSnapshots,
    parent_map: &HashMap<String, Option<String>>,
) -> DeviceHeadRelation {
    match (
        local.head_for_device(device_id),
        remote.head_for_device(device_id),
    ) {
        (Some(local_head), Some(remote_head)) if local_head == remote_head => {
            DeviceHeadRelation::Same
        }
        (Some(local_head), Some(remote_head)) => {
            if is_ancestor(local_head, remote_head, parent_map) {
                DeviceHeadRelation::LocalAhead
            } else if is_ancestor(remote_head, local_head, parent_map) {
                DeviceHeadRelation::LocalBehind
            } else {
                DeviceHeadRelation::Diverged
            }
        }
        (Some(_), None) => DeviceHeadRelation::RemoteMissing,
        (None, Some(_)) => DeviceHeadRelation::LocalMissing,
        (None, None) => DeviceHeadRelation::Same,
    }
}

/// Returns true when `ancestor` is reachable from `descendant` by following parent links.
fn is_ancestor(
    descendant: &str,
    ancestor: &str,
    parent_map: &HashMap<String, Option<String>>,
) -> bool {
    let mut current = descendant.to_string();
    let mut visited = HashSet::new();

    loop {
        if current == ancestor {
            return true;
        }
        if !visited.insert(current.clone()) {
            return false;
        }
        match parent_map.get(&current).cloned().flatten() {
            Some(parent) => current = parent,
            None => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::Snapshot;

    fn snap(date: &str, parent: Option<&str>) -> Snapshot {
        Snapshot {
            date: date.to_string(),
            describe: String::new(),
            path: String::new(),
            archive_format: crate::backup::ArchiveFormat::Zip,
            size: 0,
            parent: parent.map(|s| s.to_string()),
            archive_hash: None,
            device_id: None,
            created_by: Default::default(),
        }
    }

    fn gs(name: &str, backups: Vec<Snapshot>, heads: &[(&str, &str)]) -> GameSnapshots {
        let mut snapshots = GameSnapshots::new(name);
        snapshots.backups = backups;
        for (device_id, head) in heads {
            snapshots.set_head_for_device((*device_id).to_string(), Some((*head).to_string()));
        }
        snapshots
    }

    #[test]
    fn both_empty_is_in_sync() {
        let local = gs("g", vec![], &[]);
        let remote = gs("g", vec![], &[]);
        assert_eq!(
            determine_sync_relation(&local, &remote),
            SyncRelation::InSync
        );
    }

    #[test]
    fn remote_only_history_is_current_device_behind() {
        let local = gs("g", vec![], &[]);
        let remote = gs("g", vec![snap("a", None)], &[("remote-device", "a")]);
        assert_eq!(
            determine_sync_relation(&local, &remote),
            SyncRelation::CurrentDeviceBehind
        );
    }

    #[test]
    fn local_only_history_is_current_device_ahead() {
        let current = get_current_device_id().clone();
        let local = gs("g", vec![snap("a", None)], &[(current.as_str(), "a")]);
        let remote = gs("g", vec![], &[]);
        assert_eq!(
            determine_sync_relation(&local, &remote),
            SyncRelation::CurrentDeviceAhead
        );
    }

    #[test]
    fn same_graph_same_heads_is_in_sync() {
        let current = get_current_device_id().clone();
        let snaps = vec![snap("a", None), snap("b", Some("a"))];
        let local = gs("g", snaps.clone(), &[(current.as_str(), "b")]);
        let remote = gs("g", snaps, &[(current.as_str(), "b")]);
        assert_eq!(
            determine_sync_relation(&local, &remote),
            SyncRelation::InSync
        );
    }

    #[test]
    fn current_device_head_metadata_only_can_be_ahead() {
        let current = get_current_device_id().clone();
        let snaps = vec![snap("a", None), snap("b", Some("a"))];
        let local = gs("g", snaps.clone(), &[(current.as_str(), "b")]);
        let remote = gs("g", snaps, &[(current.as_str(), "a")]);
        assert_eq!(
            determine_sync_relation(&local, &remote),
            SyncRelation::CurrentDeviceAhead
        );
    }

    #[test]
    fn diverged_shared_tree_is_not_conflict() {
        let current = get_current_device_id().clone();
        let local = gs(
            "g",
            vec![snap("a", None), snap("b", Some("a"))],
            &[(current.as_str(), "b")],
        );
        let remote = gs(
            "g",
            vec![snap("a", None), snap("c", Some("a"))],
            &[("remote-device", "c")],
        );
        assert_eq!(
            determine_sync_relation(&local, &remote),
            SyncRelation::SharedTreeDiverged
        );
    }

    #[test]
    fn disjoint_trees_are_parallel_branches() {
        let current = get_current_device_id().clone();
        let local = gs("g", vec![snap("x", None)], &[(current.as_str(), "x")]);
        let remote = gs("g", vec![snap("y", None)], &[("remote-device", "y")]);
        assert_eq!(
            determine_sync_relation(&local, &remote),
            SyncRelation::ParallelBranches
        );
    }

    #[test]
    fn same_device_diverging_heads_are_incompatible() {
        let current = get_current_device_id().clone();
        let local = gs(
            "g",
            vec![snap("a", None), snap("b", Some("a"))],
            &[(current.as_str(), "b")],
        );
        let remote = gs(
            "g",
            vec![snap("a", None), snap("c", Some("a"))],
            &[(current.as_str(), "c")],
        );
        assert_eq!(
            determine_sync_relation(&local, &remote),
            SyncRelation::IncompatibleState
        );
    }
}
