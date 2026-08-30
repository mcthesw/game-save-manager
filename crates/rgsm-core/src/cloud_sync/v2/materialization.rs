use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use opendal::Operator;
use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use super::{
    ArchiveIntegrity, ArchiveIntegrityError, CLOUD_MANIFEST_PATH, CloudArchiveDeletionView,
    CloudManifest, CloudManifestRepository, DeletionRegistryError, DeletionRegistryRepository,
    ManifestError, ManifestRepositoryError, MaterializationOutcome, MaterializationPreview,
    SnapshotDeletionLifecycle, SnapshotDeletionLifecycleError, SnapshotState, cloud_archive_path,
};
use crate::backup::{ArchiveFormat, CreatedBy, Snapshot, archive_path};
use crate::cloud_sync::transfer::{CloudTransfer, replace_path_preserving_existing};
use crate::config::SyncMode;
use crate::device::{DeviceId, encode_device_id};
use crate::preclude::BackendError;

const MATERIALIZATION_PROGRESS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type, utoipa::ToSchema)]
pub struct CloudArchiveLibraryView {
    pub games: Vec<CloudArchiveGameView>,
    pub pending_materialization: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type, utoipa::ToSchema)]
pub struct CloudArchiveGameView {
    pub game_id: String,
    pub name: String,
    pub sync_mode: SyncMode,
    pub cloud_sync_enabled: bool,
    pub live_save_process_name: Option<String>,
    pub live_save_snapshot_on_exit: bool,
    pub retention_limit: Option<u32>,
    pub managed: bool,
    pub visible: bool,
    pub advertised_head_count: usize,
    pub requires_choice: bool,
    /// True when a remote head is strictly ahead of the local head on the
    /// same branch (no divergence, just newer progress available).
    pub has_update: bool,
    pub snapshots: Vec<CloudArchiveSnapshotView>,
    pub pending_deletions: Vec<CloudArchiveDeletionView>,
    pub local_count: usize,
    pub cloud_count: usize,
    pub local_only_count: usize,
    pub cloud_only_count: usize,
    pub both_available_count: usize,
    pub other_device_only_count: usize,
    pub unavailable_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type, utoipa::ToSchema)]
pub struct CloudArchiveSnapshotView {
    pub snapshot_id: String,
    pub description: String,
    pub size: Option<u64>,
    pub local_evidence: LocalArchiveEvidence,
    pub cloud_verified: bool,
    pub reported_on_devices: Vec<DeviceId>,
    pub created_by: CreatedBy,
    pub retention_protected: bool,
    pub parent: Option<String>,
}

/// Evidence available while building the cloud catalog view of a local archive.
/// This intentionally does not claim content verification: catalog views use a
/// cheap metadata check, while transfer operations perform the full hash check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum LocalArchiveEvidence {
    /// Cloud metadata cannot confirm or reject the local catalog entry.
    Unknown,
    /// A regular file with the expected size is present at the local archive
    /// path.
    Present,
    /// The local file is missing, is not a regular file, or has a different
    /// size.
    Mismatch,
}

impl LocalArchiveEvidence {
    fn is_present(self) -> bool {
        self == Self::Present
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MaterializationPlan {
    schema_version: u32,
    manifest_revision: u64,
    scope: Option<String>,
    max_catalog_revision: Option<u64>,
    #[serde(default)]
    min_catalog_revision_exclusive: Option<u64>,
    remaining: Vec<MaterializationItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MaterializationItem {
    game_id: String,
    snapshot_id: String,
    archive_format: ArchiveFormat,
    integrity: ArchiveIntegrity,
}

pub struct CloudArchiveMaterializer {
    operator: Operator,
    local_archive_root: PathBuf,
    current_device_id: DeviceId,
    progress_path: PathBuf,
    max_attempts: usize,
}

impl CloudArchiveMaterializer {
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

    pub async fn view(
        &self,
        game_names: &BTreeMap<String, String>,
        local_heads: &BTreeMap<String, Option<String>>,
    ) -> Result<CloudArchiveLibraryView, MaterializationError> {
        self.converge_local_tombstones().await?;
        let manifest = self.repository().load().await?;
        let mut games = Vec::with_capacity(manifest.games.len());
        for (game_id, game) in &manifest.games {
            let mut snapshots = Vec::new();
            let mut pending_deletions = Vec::new();
            for node in game.snapshots.values() {
                if let SnapshotState::PendingTombstone(pending) = &node.state {
                    pending_deletions.push(CloudArchiveDeletionView {
                        snapshot_id: node.snapshot_id.clone(),
                        description: node.description.clone(),
                        retryable: pending.acting_device == self.current_device_id,
                    });
                }
            }
            for node in game.snapshots.values().filter(|node| node.state.is_live()) {
                let SnapshotState::Live(live) = &node.state else {
                    unreachable!("live filter guarantees a live Snapshot")
                };
                let local_evidence = classify_local_archive_evidence(
                    live.integrity.as_ref(),
                    &self.local_path(game_id, &node.snapshot_id, node.archive_format),
                );
                let reported_on_devices = game
                    .local_archives
                    .iter()
                    .filter(|(device, snapshots)| {
                        *device != &self.current_device_id && snapshots.contains(&node.snapshot_id)
                    })
                    .map(|(device, _)| device.clone())
                    .collect();
                snapshots.push(CloudArchiveSnapshotView {
                    snapshot_id: node.snapshot_id.clone(),
                    description: node.description.clone(),
                    size: live.integrity.as_ref().map(|integrity| integrity.size),
                    local_evidence,
                    cloud_verified: live.cloud_archive_verified,
                    reported_on_devices,
                    created_by: live.created_by.clone(),
                    retention_protected: live.retention_protected,
                    parent: node.parent.clone(),
                });
            }
            snapshots.reverse();
            games.push(CloudArchiveGameView {
                game_id: game_id.clone(),
                name: game_names
                    .get(game_id)
                    .cloned()
                    .unwrap_or_else(|| game_id.clone()),
                sync_mode: SyncMode::Manual,
                cloud_sync_enabled: false,
                live_save_process_name: None,
                live_save_snapshot_on_exit: false,
                retention_limit: None,
                managed: false,
                visible: false,
                advertised_head_count: game
                    .device_heads
                    .values()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                requires_choice: {
                    let local_head = local_heads.get(game_id).and_then(|head| head.as_deref());
                    let mut heads = game
                        .device_heads
                        .iter()
                        .filter(|(device, _)| {
                            // Exclude current device's stale advertised head
                            // when a local Current Position is authoritative.
                            !(**device == self.current_device_id && local_head.is_some())
                        })
                        .map(|(_, head)| head.clone())
                        .collect::<std::collections::BTreeSet<_>>();
                    if let Some(local) = local_head {
                        heads.insert(local.to_string());
                    }
                    let list: Vec<_> = heads.into_iter().collect();
                    list.iter().any(|left| {
                        list.iter().any(|right| {
                            left != right
                                && !game.is_ancestor_or_equal(left, right).unwrap_or(true)
                                && !game.is_ancestor_or_equal(right, left).unwrap_or(true)
                        })
                    })
                },
                has_update: {
                    let local_head = local_heads.get(game_id).and_then(|head| head.as_deref());
                    match local_head {
                        None => !game.device_heads.is_empty(),
                        Some(local) => game
                            .device_heads
                            .iter()
                            .filter(|(device, _)| device.as_str() != self.current_device_id)
                            .any(|(_, remote)| {
                                remote.as_str() != local
                                    && game.is_ancestor_or_equal(local, remote).unwrap_or(false)
                            }),
                    }
                },
                local_count: snapshots
                    .iter()
                    .filter(|snapshot| snapshot.local_evidence.is_present())
                    .count(),
                cloud_count: snapshots
                    .iter()
                    .filter(|snapshot| snapshot.cloud_verified)
                    .count(),
                local_only_count: snapshots
                    .iter()
                    .filter(|snapshot| {
                        snapshot.local_evidence.is_present() && !snapshot.cloud_verified
                    })
                    .count(),
                cloud_only_count: snapshots
                    .iter()
                    .filter(|snapshot| {
                        !snapshot.local_evidence.is_present() && snapshot.cloud_verified
                    })
                    .count(),
                both_available_count: snapshots
                    .iter()
                    .filter(|snapshot| {
                        snapshot.local_evidence.is_present() && snapshot.cloud_verified
                    })
                    .count(),
                other_device_only_count: snapshots
                    .iter()
                    .filter(|snapshot| {
                        !snapshot.local_evidence.is_present()
                            && !snapshot.cloud_verified
                            && !snapshot.reported_on_devices.is_empty()
                    })
                    .count(),
                unavailable_count: snapshots
                    .iter()
                    .filter(|snapshot| {
                        !snapshot.local_evidence.is_present()
                            && !snapshot.cloud_verified
                            && snapshot.reported_on_devices.is_empty()
                    })
                    .count(),
                snapshots,
                pending_deletions,
            });
        }
        Ok(CloudArchiveLibraryView {
            games,
            pending_materialization: self.progress_path.exists(),
        })
    }

    pub async fn preview_materialize_all(
        &self,
    ) -> Result<MaterializationPreview, MaterializationError> {
        self.converge_local_tombstones().await?;
        let plan = match self.load_plan().await? {
            Some(plan) => plan,
            None => self.load_or_plan(None, None, None, false).await?,
        };
        Ok(MaterializationPreview {
            snapshot_count: plan.remaining.len(),
            total_bytes: plan.remaining.iter().map(|item| item.integrity.size).sum(),
        })
    }

    pub async fn catalog_revision(&self) -> Result<u64, MaterializationError> {
        Ok(self.repository().load().await?.revision)
    }

    pub async fn upload(
        &self,
        game_id: &str,
        snapshot_id: &str,
    ) -> Result<(), MaterializationError> {
        self.converge_local_tombstones().await?;
        self.ensure_active(game_id).await?;
        let manifest = self.repository().load().await?;
        let item = manifest_item(&manifest, game_id, snapshot_id, false)?;
        let local_path = self.local_path(game_id, snapshot_id, item.archive_format);
        verify_file(item.integrity.clone(), local_path.clone()).await?;
        let remote_path = cloud_archive_path(game_id, snapshot_id, item.archive_format)?;
        let verify_path = self.staging_path(&item, "upload-verify");
        let transfer = CloudTransfer::new(&self.operator);
        let mut verified = false;
        for _ in 0..self.max_attempts {
            transfer
                .upload_file_streaming(&local_path, &remote_path)
                .await?;
            transfer
                .download_file_streaming(&remote_path, &verify_path)
                .await?;
            if verify_file(item.integrity.clone(), verify_path.clone())
                .await
                .is_ok()
            {
                verified = true;
                break;
            }
        }
        remove_file_if_exists(&verify_path).await?;
        if !verified {
            return Err(MaterializationError::CloudVerificationFailed {
                game_id: game_id.to_string(),
                snapshot_id: snapshot_id.to_string(),
            });
        }
        if let Err(error) = self.ensure_active(game_id).await {
            if matches!(
                &error,
                MaterializationError::DeletionRegistry(DeletionRegistryError::GameDeleted(_))
            ) {
                self.operator
                    .delete(&remote_path)
                    .await
                    .map_err(BackendError::from)?;
            }
            return Err(error);
        }
        let current_device = self.current_device_id.clone();
        let final_game_id = game_id.to_string();
        let manifest_game_id = final_game_id.clone();
        let snapshot_id = snapshot_id.to_string();
        self.repository()
            .mutate(move |manifest| {
                let game = manifest
                    .games
                    .get_mut(&manifest_game_id)
                    .ok_or_else(|| ManifestError::MissingGame(manifest_game_id.clone()))?;
                let node = game
                    .snapshots
                    .get_mut(&snapshot_id)
                    .ok_or_else(|| ManifestError::MissingSnapshot(snapshot_id.clone()))?;
                let SnapshotState::Live(live) = &mut node.state else {
                    return Err(ManifestError::ExpectedLive(snapshot_id.clone()));
                };
                if live.integrity.as_ref() != Some(&item.integrity) {
                    return Err(ManifestError::InvalidIntegrity(snapshot_id.clone()));
                }
                live.cloud_archive_verified = true;
                live.cloud_evicted = false;
                game.report_local_archive(current_device.clone(), snapshot_id.clone(), true);
                Ok(())
            })
            .await?;
        if let Err(error) = self.ensure_active(&final_game_id).await {
            if matches!(
                &error,
                MaterializationError::DeletionRegistry(DeletionRegistryError::GameDeleted(_))
            ) {
                self.operator
                    .delete(&remote_path)
                    .await
                    .map_err(BackendError::from)?;
            }
            return Err(error);
        }
        Ok(())
    }

    pub async fn download(
        &self,
        game_id: &str,
        snapshot_id: &str,
    ) -> Result<Vec<Snapshot>, MaterializationError> {
        self.converge_local_tombstones().await?;
        let manifest = self.repository().load().await?;
        let item = manifest_item(&manifest, game_id, snapshot_id, true)?;
        self.download_item(&item).await?;
        self.imported_lineage(&manifest, game_id, snapshot_id)
    }

    pub fn imported_lineage(
        &self,
        manifest: &CloudManifest,
        game_id: &str,
        snapshot_id: &str,
    ) -> Result<Vec<Snapshot>, MaterializationError> {
        let game = manifest
            .games
            .get(game_id)
            .ok_or_else(|| MaterializationError::GameNotFound(game_id.to_string()))?;
        let mut lineage = Vec::new();
        let mut visited = HashSet::new();
        let mut cursor = snapshot_id;
        loop {
            if !visited.insert(cursor.to_string()) {
                return Err(MaterializationError::SnapshotUnavailable(
                    cursor.to_string(),
                ));
            }
            let node = game
                .snapshots
                .get(cursor)
                .ok_or_else(|| MaterializationError::SnapshotNotFound(cursor.to_string()))?;
            let SnapshotState::Live(live) = &node.state else {
                return Err(MaterializationError::SnapshotUnavailable(
                    cursor.to_string(),
                ));
            };
            lineage.push(Snapshot {
                date: node.snapshot_id.clone(),
                describe: node.description.clone(),
                path: self
                    .local_path(game_id, &node.snapshot_id, node.archive_format)
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

    pub async fn imported_local_catalog(
        &self,
    ) -> Result<BTreeMap<String, Vec<Snapshot>>, MaterializationError> {
        let manifest = self.repository().load().await?;
        let mut imported = BTreeMap::new();
        for (game_id, game) in &manifest.games {
            let Some(reported) = game.local_archives.get(&self.current_device_id) else {
                continue;
            };
            let mut snapshots = Vec::new();
            for snapshot_id in reported {
                snapshots.extend(self.imported_lineage(&manifest, game_id, snapshot_id)?);
            }
            if !snapshots.is_empty() {
                imported.insert(game_id.clone(), snapshots);
            }
        }
        Ok(imported)
    }

    pub async fn materialize_all(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<MaterializationOutcome, MaterializationError> {
        self.converge_local_tombstones().await?;
        match self.load_plan().await? {
            Some(plan) => {
                self.materialize_scope(
                    plan.scope,
                    plan.max_catalog_revision,
                    plan.min_catalog_revision_exclusive,
                    cancellation,
                )
                .await
            }
            None => self.materialize_scope(None, None, None, cancellation).await,
        }
    }

    pub async fn preview_game(
        &self,
        game_id: &str,
        max_catalog_revision: u64,
    ) -> Result<MaterializationPreview, MaterializationError> {
        self.converge_local_tombstones().await?;
        let plan = self
            .load_or_plan(
                Some(game_id.to_string()),
                Some(max_catalog_revision),
                None,
                false,
            )
            .await?;
        Ok(MaterializationPreview {
            snapshot_count: plan.remaining.len(),
            total_bytes: plan.remaining.iter().map(|item| item.integrity.size).sum(),
        })
    }

    pub async fn materialize_game(
        &self,
        game_id: &str,
        max_catalog_revision: u64,
        cancellation: &CancellationToken,
    ) -> Result<MaterializationOutcome, MaterializationError> {
        self.converge_local_tombstones().await?;
        self.materialize_scope(
            Some(game_id.to_string()),
            Some(max_catalog_revision),
            None,
            cancellation,
        )
        .await
    }

    pub async fn materialize_game_since(
        &self,
        game_id: &str,
        min_catalog_revision_exclusive: u64,
        cancellation: &CancellationToken,
    ) -> Result<MaterializationOutcome, MaterializationError> {
        self.converge_local_tombstones().await?;
        self.materialize_scope(
            Some(game_id.to_string()),
            None,
            Some(min_catalog_revision_exclusive),
            cancellation,
        )
        .await
    }

    pub async fn resume_pending(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<Option<MaterializationOutcome>, MaterializationError> {
        self.converge_local_tombstones().await?;
        let Some(plan) = self.load_plan().await? else {
            return Ok(None);
        };
        Ok(Some(
            self.materialize_scope(
                plan.scope,
                plan.max_catalog_revision,
                plan.min_catalog_revision_exclusive,
                cancellation,
            )
            .await?,
        ))
    }

    pub async fn delete_snapshot(
        &self,
        game_id: &str,
        snapshot_id: &str,
        confirmed: bool,
    ) -> Result<(), MaterializationError> {
        self.converge_local_tombstones().await?;
        self.deletion_lifecycle()
            .delete_snapshot(game_id, snapshot_id, confirmed)
            .await?;
        self.converge_local_tombstones().await?;
        Ok(())
    }

    /// Delete local Archive copies for every durable Tombstone before another
    /// transfer is planned. Returns the structural Snapshot IDs so the service
    /// layer can remove their legacy local presentation without reparenting.
    pub async fn converge_local_tombstones(
        &self,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, MaterializationError> {
        let tombstones = self
            .deletion_lifecycle()
            .converge_local_tombstones()
            .await?;
        if let Some(mut plan) = self.load_plan().await? {
            plan.remaining.retain(|item| {
                !tombstones
                    .get(&item.game_id)
                    .is_some_and(|items| items.contains(&item.snapshot_id))
            });
            if plan.remaining.is_empty() {
                remove_file_if_exists(&self.progress_path).await?;
            } else {
                self.store_plan(&plan).await?;
            }
        }
        Ok(tombstones)
    }

    async fn materialize_scope(
        &self,
        scope: Option<String>,
        max_catalog_revision: Option<u64>,
        min_catalog_revision_exclusive: Option<u64>,
        cancellation: &CancellationToken,
    ) -> Result<MaterializationOutcome, MaterializationError> {
        let mut plan = self
            .load_or_plan(
                scope,
                max_catalog_revision,
                min_catalog_revision_exclusive,
                true,
            )
            .await?;
        let initial = plan.remaining.len();
        while let Some(item) = plan.remaining.first().cloned() {
            if cancellation.is_cancelled() {
                return Err(MaterializationError::Cancelled);
            }
            self.download_item(&item).await?;
            plan.remaining.remove(0);
            self.store_plan(&plan).await?;
        }
        remove_file_if_exists(&self.progress_path).await?;
        Ok(MaterializationOutcome {
            downloaded: initial,
            remaining: 0,
        })
    }

    async fn download_item(&self, item: &MaterializationItem) -> Result<(), MaterializationError> {
        let latest = self.repository().load().await?;
        if manifest_item(&latest, &item.game_id, &item.snapshot_id, true)? != *item {
            return Err(MaterializationError::TargetChanged(
                item.snapshot_id.clone(),
            ));
        }
        let remote_path =
            cloud_archive_path(&item.game_id, &item.snapshot_id, item.archive_format)?;
        let staging_path = self.staging_path(item, "download");
        let local_path = self.local_path(&item.game_id, &item.snapshot_id, item.archive_format);
        CloudTransfer::new(&self.operator)
            .download_file_streaming(&remote_path, &staging_path)
            .await?;
        if let Err(error) = verify_file(item.integrity.clone(), staging_path.clone()).await {
            remove_file_if_exists(&staging_path).await?;
            return Err(error);
        }
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        replace_path_preserving_existing(&staging_path, &local_path).await?;
        let current_device = self.current_device_id.clone();
        let game_id = item.game_id.clone();
        let snapshot_id = item.snapshot_id.clone();
        self.repository()
            .mutate(move |manifest| {
                let game = manifest
                    .games
                    .get_mut(&game_id)
                    .ok_or_else(|| ManifestError::MissingGame(game_id.clone()))?;
                game.report_local_archive(current_device.clone(), snapshot_id.clone(), true);
                Ok(())
            })
            .await?;
        Ok(())
    }

    async fn load_or_plan(
        &self,
        scope: Option<String>,
        max_catalog_revision: Option<u64>,
        min_catalog_revision_exclusive: Option<u64>,
        persist_new: bool,
    ) -> Result<MaterializationPlan, MaterializationError> {
        let saved_plan = self.load_plan().await?;
        if saved_plan.as_ref().is_some_and(|plan| {
            plan.scope != scope
                || plan.max_catalog_revision != max_catalog_revision
                || plan.min_catalog_revision_exclusive != min_catalog_revision_exclusive
        }) {
            return Err(MaterializationError::AnotherMaterializationPending);
        }
        let manifest = self.repository().load().await?;
        let mut remaining = Vec::new();
        if let Some(saved_plan) = saved_plan {
            for saved in saved_plan.remaining {
                let Some(game) = manifest.games.get(&saved.game_id) else {
                    continue;
                };
                let Some(node) = game.snapshots.get(&saved.snapshot_id) else {
                    continue;
                };
                let SnapshotState::Live(live) = &node.state else {
                    continue;
                };
                let Some(integrity) = &live.integrity else {
                    continue;
                };
                let local_archive_verified = self.is_locally_reported(game, &node.snapshot_id)
                    && verify_file(
                        integrity.clone(),
                        self.local_path(&saved.game_id, &node.snapshot_id, node.archive_format),
                    )
                    .await
                    .is_ok();
                if !live.cloud_archive_verified || local_archive_verified {
                    continue;
                }
                remaining.push(MaterializationItem {
                    game_id: saved.game_id,
                    snapshot_id: node.snapshot_id.clone(),
                    archive_format: node.archive_format,
                    integrity: integrity.clone(),
                });
            }
        } else {
            for (game_id, game) in &manifest.games {
                if scope.as_ref().is_some_and(|scope| scope != game_id) {
                    continue;
                }
                for node in game.snapshots.values() {
                    let SnapshotState::Live(live) = &node.state else {
                        continue;
                    };
                    let Some(integrity) = &live.integrity else {
                        continue;
                    };
                    let local_archive_verified = self.is_locally_reported(game, &node.snapshot_id)
                        && verify_file(
                            integrity.clone(),
                            self.local_path(game_id, &node.snapshot_id, node.archive_format),
                        )
                        .await
                        .is_ok();
                    if !live.cloud_archive_verified
                        || max_catalog_revision
                            .is_some_and(|maximum| node.catalog_revision > maximum)
                        || min_catalog_revision_exclusive
                            .is_some_and(|minimum| node.catalog_revision <= minimum)
                        || local_archive_verified
                    {
                        continue;
                    }
                    remaining.push(MaterializationItem {
                        game_id: game_id.clone(),
                        snapshot_id: node.snapshot_id.clone(),
                        archive_format: node.archive_format,
                        integrity: integrity.clone(),
                    });
                }
            }
        }
        let plan = MaterializationPlan {
            schema_version: MATERIALIZATION_PROGRESS_SCHEMA_VERSION,
            manifest_revision: manifest.revision,
            scope,
            max_catalog_revision,
            min_catalog_revision_exclusive,
            remaining,
        };
        if persist_new {
            self.store_plan(&plan).await?;
        }
        Ok(plan)
    }

    async fn load_plan(&self) -> Result<Option<MaterializationPlan>, MaterializationError> {
        let bytes = match tokio::fs::read(&self.progress_path).await {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let plan: MaterializationPlan = serde_json::from_slice(&bytes)?;
        if plan.schema_version != MATERIALIZATION_PROGRESS_SCHEMA_VERSION {
            return Err(MaterializationError::UnsupportedProgressSchema(
                plan.schema_version,
            ));
        }
        Ok(Some(plan))
    }

    async fn store_plan(&self, plan: &MaterializationPlan) -> Result<(), MaterializationError> {
        CloudTransfer::new(&self.operator)
            .write_local_bytes_atomically(&self.progress_path, &serde_json::to_vec_pretty(plan)?)
            .await?;
        Ok(())
    }

    fn repository(&self) -> CloudManifestRepository<super::OpenDalManifestTransport> {
        CloudManifestRepository::new(
            self.operator.clone(),
            CLOUD_MANIFEST_PATH,
            self.max_attempts,
        )
    }

    async fn ensure_active(&self, game_id: &str) -> Result<(), MaterializationError> {
        DeletionRegistryRepository::new(self.operator.clone(), self.max_attempts)
            .ensure_active(&self.current_device_id, game_id)
            .await?;
        Ok(())
    }

    fn deletion_lifecycle(&self) -> SnapshotDeletionLifecycle {
        SnapshotDeletionLifecycle::new(
            self.operator.clone(),
            self.local_archive_root.clone(),
            self.current_device_id.clone(),
            self.max_attempts,
        )
    }

    fn is_locally_reported(&self, game: &super::GameManifest, snapshot_id: &str) -> bool {
        game.local_archives
            .get(&self.current_device_id)
            .is_some_and(|snapshots| snapshots.contains(snapshot_id))
    }

    fn local_path(&self, game_id: &str, snapshot_id: &str, format: ArchiveFormat) -> PathBuf {
        archive_path(&self.local_archive_root.join(game_id), snapshot_id, format)
    }

    fn staging_path(&self, item: &MaterializationItem, suffix: &str) -> PathBuf {
        self.progress_path.with_extension("staging").join(format!(
            "{}-{}-{suffix}.tmp",
            encode_device_id(&item.game_id),
            encode_device_id(&item.snapshot_id)
        ))
    }
}

fn manifest_item(
    manifest: &CloudManifest,
    game_id: &str,
    snapshot_id: &str,
    require_cloud: bool,
) -> Result<MaterializationItem, MaterializationError> {
    let game = manifest
        .games
        .get(game_id)
        .ok_or_else(|| MaterializationError::GameNotFound(game_id.to_string()))?;
    let node = game
        .snapshots
        .get(snapshot_id)
        .ok_or_else(|| MaterializationError::SnapshotNotFound(snapshot_id.to_string()))?;
    let SnapshotState::Live(live) = &node.state else {
        return Err(MaterializationError::SnapshotUnavailable(
            snapshot_id.to_string(),
        ));
    };
    let integrity = live
        .integrity
        .clone()
        .ok_or_else(|| MaterializationError::SnapshotUnavailable(snapshot_id.to_string()))?;
    if require_cloud && !live.cloud_archive_verified {
        return Err(MaterializationError::CloudArchiveUnavailable(
            snapshot_id.to_string(),
        ));
    }
    Ok(MaterializationItem {
        game_id: game_id.to_string(),
        snapshot_id: snapshot_id.to_string(),
        archive_format: node.archive_format,
        integrity,
    })
}

pub(super) async fn verify_file(
    integrity: ArchiveIntegrity,
    path: PathBuf,
) -> Result<(), MaterializationError> {
    tokio::task::spawn_blocking(move || integrity.verify_file(&path))
        .await
        .map_err(|error| MaterializationError::HashTask(error.to_string()))??;
    Ok(())
}

fn local_size_matches(path: &Path, expected_size: u64) -> bool {
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() == expected_size)
}

fn classify_local_archive_evidence(
    integrity: Option<&ArchiveIntegrity>,
    local_path: &Path,
) -> LocalArchiveEvidence {
    let Some(integrity) = integrity else {
        return LocalArchiveEvidence::Unknown;
    };
    if local_size_matches(local_path, integrity.size) {
        LocalArchiveEvidence::Present
    } else {
        LocalArchiveEvidence::Mismatch
    }
}

#[cfg(test)]
mod local_archive_evidence_tests {
    use super::*;

    fn integrity(size: u64) -> ArchiveIntegrity {
        ArchiveIntegrity {
            size,
            xxh3_64: "0000000000000000".to_string(),
        }
    }

    #[test]
    fn metadata_without_integrity_is_unknown() {
        let temp = temp_dir::TempDir::new().unwrap();
        let archive = temp.path().join("snapshot.7z");
        std::fs::write(&archive, b"save").unwrap();

        assert_eq!(
            classify_local_archive_evidence(None, &archive),
            LocalArchiveEvidence::Unknown
        );
    }

    #[test]
    fn file_evidence_does_not_depend_on_manifest_ownership() {
        let temp = temp_dir::TempDir::new().unwrap();
        let archive = temp.path().join("snapshot.7z");
        std::fs::write(&archive, b"save").unwrap();

        assert_eq!(
            classify_local_archive_evidence(Some(&integrity(4)), &archive),
            LocalArchiveEvidence::Present
        );
        assert_eq!(
            classify_local_archive_evidence(Some(&integrity(5)), &archive),
            LocalArchiveEvidence::Mismatch
        );
        assert_eq!(
            classify_local_archive_evidence(Some(&integrity(4)), &temp.path().join("missing.7z")),
            LocalArchiveEvidence::Mismatch
        );
    }

    #[test]
    fn reported_archive_size_distinguishes_presence_from_mismatch() {
        let temp = temp_dir::TempDir::new().unwrap();
        let archive = temp.path().join("snapshot.7z");
        std::fs::write(&archive, b"save").unwrap();

        assert_eq!(
            classify_local_archive_evidence(Some(&integrity(4)), &archive),
            LocalArchiveEvidence::Present
        );
        assert_eq!(
            classify_local_archive_evidence(Some(&integrity(5)), &archive),
            LocalArchiveEvidence::Mismatch
        );
        assert_eq!(
            classify_local_archive_evidence(Some(&integrity(4)), &temp.path().join("missing.7z")),
            LocalArchiveEvidence::Mismatch
        );
    }
}

async fn remove_file_if_exists(path: &Path) -> Result<(), std::io::Error> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[derive(Debug, Error)]
pub enum MaterializationError {
    #[error("V2 Game not found: {0}")]
    GameNotFound(String),
    #[error("V2 Snapshot not found: {0}")]
    SnapshotNotFound(String),
    #[error("Snapshot is deleted or has no verified identity: {0}")]
    SnapshotUnavailable(String),
    #[error("Snapshot is known but its Cloud Archive is not available: {0}")]
    CloudArchiveUnavailable(String),
    #[error("Snapshot changed while its Archive was being materialized: {0}")]
    TargetChanged(String),
    #[error("Cloud Archive verification failed for {game_id}/{snapshot_id}")]
    CloudVerificationFailed {
        game_id: String,
        snapshot_id: String,
    },
    #[error("Unsupported materialization progress schema: {0}")]
    UnsupportedProgressSchema(u32),
    #[error("Materialization was cancelled")]
    Cancelled,
    #[error("Another bounded materialization operation must finish or be cancelled first")]
    AnotherMaterializationPending,
    #[error("Archive hash task failed: {0}")]
    HashTask(String),
    #[error(transparent)]
    Integrity(#[from] ArchiveIntegrityError),
    #[error(transparent)]
    Manifest(#[from] ManifestRepositoryError),
    #[error(transparent)]
    Deletion(#[from] SnapshotDeletionLifecycleError),
    #[error(transparent)]
    DeletionRegistry(#[from] DeletionRegistryError),
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error("Materialization I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Materialization progress serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
