use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::backup::Snapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum UpgradeIssueKind {
    MissingArchive,
    UnreadableArchive,
    AssociationRequired,
    MultipleInstances,
    HistoryChanged,
    OriginalChanged,
    CatalogUnreadable,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeIssue {
    pub kind: UpgradeIssueKind,
    pub archive_entry: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpgradePendingItem {
    pub id: String,
    pub game_id: String,
    pub game_name: String,
    pub snapshot_id: String,
    pub source_path: String,
    pub issue: UpgradeIssue,
    pub save_units: Vec<UpgradeUnitChoice>,
}

#[derive(Debug, Clone, Serialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeUnitChoice {
    pub id: u32,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LocalUpgradeView {
    pub started: bool,
    pub total: usize,
    pub remaining: usize,
    pub completed: usize,
    pub original_count: usize,
    pub original_bytes: u64,
    pub estimated_extra_bytes: u64,
    pub unknown_sizes: usize,
    pub pending: Vec<UpgradePendingItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeRetry {
    pub item_id: String,
    pub replacement_path: Option<String>,
    pub archive_entry: Option<String>,
    pub save_unit_id: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum UpgradeItemState {
    Queued,
    Prepared,
    Completed,
    Cleaned,
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UpgradeItem {
    pub id: String,
    pub game_id: String,
    pub game_name: String,
    pub snapshot: Snapshot,
    pub source_path: String,
    pub retained_path: String,
    pub extra_backup: bool,
    pub replacement_path: Option<String>,
    #[serde(deserialize_with = "crate::backup::deserialize_archive_name")]
    pub output_name: Option<String>,
    pub original_size: u64,
    pub unpacked_size: Option<u64>,
    pub original_hash: Option<String>,
    pub output_hash: Option<String>,
    pub output_size: u64,
    pub state: UpgradeItemState,
    pub issue: Option<UpgradeIssue>,
    pub associations: BTreeMap<String, u32>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UpgradeJournal {
    pub schema_version: u32,
    pub started: bool,
    pub items: Vec<UpgradeItem>,
}
