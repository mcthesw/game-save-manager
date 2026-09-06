//! Session-only acknowledgements. Neither cloud metadata nor live saves are mutated.
use std::collections::{BTreeMap, BTreeSet};

use rgsm_core::cloud_sync::v2::{ProgressRelation, V2ConflictReview};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ProgressNotice {
    pub id: String,
    pub game_id: String,
    pub game_name: String,
}

#[derive(Debug, Clone, Default, Serialize, ToSchema)]
pub struct PendingProgress {
    pub library_id: String,
    pub notices: Vec<ProgressNotice>,
}

struct Entry {
    remote_heads: BTreeSet<String>,
    notice: ProgressNotice,
    deferred: bool,
}

#[derive(Default)]
pub struct ProgressNoticeBook {
    library_id: String,
    entries: BTreeMap<String, Entry>,
}

impl ProgressNoticeBook {
    pub fn retain_games(&mut self, library_id: &str, eligible: &BTreeSet<String>) {
        if self.library_id != library_id {
            self.entries.clear();
            self.library_id = library_id.to_string();
        }
        self.entries.retain(|id, _| eligible.contains(id));
    }

    /// Returns true only for newly observed progress, not renames or metadata revisions.
    pub fn observe(
        &mut self,
        game_name: &str,
        current_device: &str,
        review: &V2ConflictReview,
    ) -> bool {
        let remote = review
            .candidates
            .iter()
            .filter(|candidate| {
                candidate.cloud_available
                    && candidate.devices.iter().any(|id| id != current_device)
                    && matches!(
                        candidate.relation,
                        ProgressRelation::RemoteAhead
                            | ProgressRelation::DifferentProgress
                            | ProgressRelation::NoLocalPosition
                    )
            })
            .map(|candidate| candidate.snapshot_id.clone())
            .collect::<BTreeSet<_>>();
        if remote.is_empty() {
            self.entries.remove(&review.game_id);
            return false;
        }
        if let Some(entry) = self.entries.get_mut(&review.game_id)
            && remote.is_subset(&entry.remote_heads)
        {
            entry.remote_heads = remote;
            entry.notice.game_name = game_name.to_string();
            return false;
        }
        self.entries.insert(
            review.game_id.clone(),
            Entry {
                remote_heads: remote,
                notice: ProgressNotice {
                    id: format!("{:032x}", rand::random::<u128>()),
                    game_id: review.game_id.clone(),
                    game_name: game_name.to_string(),
                },
                deferred: false,
            },
        );
        true
    }

    pub fn defer(&mut self, notice_ids: &[String]) {
        for entry in self.entries.values_mut() {
            if notice_ids.contains(&entry.notice.id) {
                entry.deferred = true;
            }
        }
    }

    pub fn pending(&self) -> PendingProgress {
        PendingProgress {
            library_id: self.library_id.clone(),
            notices: self
                .entries
                .values()
                .filter(|entry| !entry.deferred)
                .map(|entry| entry.notice.clone())
                .collect(),
        }
    }
}

pub fn should_notify(desktop_enabled: bool, window_focused: bool, new_progress: bool) -> bool {
    desktop_enabled && !window_focused && new_progress
}

#[cfg(test)]
#[path = "progress_notices_tests.rs"]
mod tests;
