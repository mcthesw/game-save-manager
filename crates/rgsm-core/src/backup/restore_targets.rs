use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use super::{CaptureSourceKind, RestoreEntry, RestorePlanError};

/// Reject overlapping writes before the archive backend can clear any target.
pub(super) fn validate_targets(entries: &[RestoreEntry]) -> Result<(), RestorePlanError> {
    let mut targets = BTreeMap::new();
    for entry in entries {
        let registry = entry.kind == CaptureSourceKind::Registry;
        let key = target_key(&entry.target_path, registry);
        if targets.insert((registry, key), entry).is_some() {
            return Err(overlap(entry));
        }
    }
    for ((registry, key), entry) in &targets {
        let mut ancestor = key.as_str();
        while let Some(separator) = ancestor.rfind('/') {
            ancestor = &ancestor[..separator];
            if targets.contains_key(&(*registry, ancestor.to_owned())) {
                return Err(overlap(entry));
            }
        }
    }
    Ok(())
}

fn overlap(entry: &RestoreEntry) -> RestorePlanError {
    RestorePlanError::OverlappingTargets {
        path: entry.target_path.to_string_lossy().into_owned(),
    }
}

fn target_key(path: &Path, registry: bool) -> String {
    let mut normalized = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir if normalized.file_name().is_some() => {
                normalized.pop();
            }
            _ => normalized.push(part),
        }
    }
    let key = normalized.to_string_lossy().replace('\\', "/");
    let key = key.trim_end_matches('/');
    if cfg!(windows) || registry {
        key.to_lowercase()
    } else {
        key.to_owned()
    }
}
