use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;

use opendal::Operator;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use super::{
    ArchiveIntegrity, ArchiveIntegrityError, CLOUD_MANIFEST_PATH, CloudArchiveMaterializer,
    CloudManifest, CloudManifestRepository, DeletionKind, ManifestError, ManifestRepositoryError,
    SnapshotDeletionLifecycle, SnapshotDeletionLifecycleError, SnapshotNode,
    SnapshotRetentionPlanner, SnapshotRetentionPlannerError, SnapshotState,
};
use crate::backup::{GameSnapshots, Snapshot, archive_path};
use crate::device::DeviceId;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SnapshotReconciliationOutcome {
    pub published: usize,
    pub uploaded: usize,
    pub downloaded: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetentionEnforcementOutcome {
    pub deleted: usize,
    pub tombstones: BTreeSet<String>,
}

pub struct SnapshotSyncCoordinator {
    operator: Operator,
    materializer: CloudArchiveMaterializer,
    local_archive_root: PathBuf,
    current_device_id: DeviceId,
    max_attempts: usize,
}

impl SnapshotSyncCoordinator {
    pub fn new(
        operator: Operator,
        local_archive_root: PathBuf,
        current_device_id: DeviceId,
        progress_path: PathBuf,
        max_attempts: usize,
    ) -> Self {
        let max_attempts = max_attempts.max(1);
        Self {
            materializer: CloudArchiveMaterializer::new(
                operator.clone(),
                local_archive_root.clone(),
                current_device_id.clone(),
                progress_path,
                max_attempts,
            ),
            operator,
            local_archive_root,
            current_device_id,
            max_attempts,
        }
    }

    pub async fn reconcile_game(
        &self,
        game_id: &str,
        local: &GameSnapshots,
        activation_revision: u64,
        local_baseline: &BTreeSet<String>,
        cancellation: &CancellationToken,
    ) -> Result<SnapshotReconciliationOutcome, SnapshotSyncError> {
        let mut manifest = self.repository().load().await?;
        let order = publication_order(&manifest, game_id, local, local_baseline)?;
        let mut outcome = SnapshotReconciliationOutcome::default();

        for snapshot in order {
            if cancellation.is_cancelled() {
                return Err(SnapshotSyncError::Cancelled);
            }
            let inherited_ancestor = local_baseline.contains(&snapshot.date);
            self.publish_local_node(
                game_id,
                &snapshot,
                inherited_ancestor.then_some(activation_revision),
            )
            .await?;
            outcome.published += 1;
        }

        manifest = self.repository().load().await?;
        let locally_reported = manifest
            .games
            .get(game_id)
            .and_then(|game| game.local_archives.get(&self.current_device_id))
            .cloned()
            .unwrap_or_default();
        for snapshot in &local.backups {
            if local_baseline.contains(&snapshot.date)
                || locally_reported.contains(&snapshot.date)
                || !self.local_path(game_id, snapshot).is_file()
            {
                continue;
            }
            self.publish_local_node(game_id, snapshot, None).await?;
            outcome.published += 1;
        }

        self.publish_current_head(game_id, local).await?;
        manifest = self.repository().load().await?;
        let upload_ids = manifest
            .games
            .get(game_id)
            .map(|game| {
                game.snapshots
                    .values()
                    .filter(|node| {
                        node.catalog_revision > activation_revision
                            && !local_baseline.contains(&node.snapshot_id)
                            && game
                                .local_archives
                                .get(&self.current_device_id)
                                .is_some_and(|items| items.contains(&node.snapshot_id))
                            && matches!(
                                &node.state,
                                SnapshotState::Live(live) if !live.cloud_archive_verified
                            )
                    })
                    .map(|node| node.snapshot_id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for snapshot_id in upload_ids {
            if cancellation.is_cancelled() {
                return Err(SnapshotSyncError::Cancelled);
            }
            self.materializer.upload(game_id, &snapshot_id).await?;
            outcome.uploaded += 1;
        }

        outcome.downloaded = self
            .materializer
            .materialize_game_since(game_id, activation_revision, cancellation)
            .await?
            .downloaded;
        Ok(outcome)
    }

    pub async fn resume_pending(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<usize, SnapshotSyncError> {
        Ok(self
            .materializer
            .resume_pending(cancellation)
            .await?
            .map_or(0, |outcome| outcome.downloaded))
    }

    pub async fn converge_local_tombstones(
        &self,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, SnapshotSyncError> {
        Ok(self.materializer.converge_local_tombstones().await?)
    }

    pub async fn enforce_retention(
        &self,
        game_id: &str,
        automatic_snapshots_per_branch: u32,
    ) -> Result<RetentionEnforcementOutcome, SnapshotSyncError> {
        let lifecycle = SnapshotDeletionLifecycle::new(
            self.operator.clone(),
            self.local_archive_root.clone(),
            self.current_device_id.clone(),
            self.max_attempts,
        );
        let mut manifest = self.repository().load().await?;
        let pending = manifest
            .games
            .get(game_id)
            .map(|game| {
                game.snapshots
                    .values()
                    .filter(|node| {
                        matches!(
                            &node.state,
                            SnapshotState::PendingTombstone(pending)
                                if pending.kind == DeletionKind::Retention
                                    && pending.acting_device == self.current_device_id
                        )
                    })
                    .map(|node| node.snapshot_id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut deleted = 0;
        for snapshot_id in pending {
            lifecycle
                .delete_retention_snapshot(game_id, &snapshot_id)
                .await?;
            deleted += 1;
        }

        manifest = self.repository().load().await?;
        let Some(game) = manifest.games.get(game_id) else {
            return Ok(RetentionEnforcementOutcome::default());
        };
        let plan = SnapshotRetentionPlanner::plan(game, automatic_snapshots_per_branch as usize)?;
        for snapshot_id in &plan.candidates {
            lifecycle
                .delete_retention_snapshot(game_id, snapshot_id)
                .await?;
            deleted += 1;
        }
        let tombstones = self
            .materializer
            .converge_local_tombstones()
            .await?
            .remove(game_id)
            .unwrap_or_default();
        Ok(RetentionEnforcementOutcome {
            deleted,
            tombstones,
        })
    }

    pub(crate) async fn publish_local_node(
        &self,
        game_id: &str,
        snapshot: &Snapshot,
        inherited_revision: Option<u64>,
    ) -> Result<(), SnapshotSyncError> {
        let local_path = self.local_path(game_id, snapshot);
        let integrity = if local_path.is_file() {
            let path = local_path.clone();
            Some(
                tokio::task::spawn_blocking(move || ArchiveIntegrity::from_file(&path))
                    .await
                    .map_err(|error| SnapshotSyncError::HashTask(error.to_string()))??,
            )
        } else {
            None
        };
        let game_id = game_id.to_string();
        let snapshot = snapshot.clone();
        let current_device_id = self.current_device_id.clone();
        self.repository()
            .mutate(move |manifest| {
                let catalog_revision =
                    inherited_revision.unwrap_or(manifest.revision.checked_add(1).ok_or_else(
                        || ManifestError::SnapshotContentConflict(snapshot.date.clone()),
                    )?);
                let game = manifest.game_mut(&game_id);
                if let Some(existing) = game.snapshots.get_mut(&snapshot.date) {
                    merge_local_snapshot(existing, &snapshot, integrity.clone())?;
                } else {
                    let mut node = match integrity.clone() {
                        Some(integrity) => SnapshotNode::live(
                            &snapshot.date,
                            snapshot.parent.clone(),
                            integrity,
                            snapshot.created_by.clone(),
                        ),
                        None => SnapshotNode::unavailable(
                            &snapshot.date,
                            snapshot.parent.clone(),
                            snapshot.created_by.clone(),
                        ),
                    };
                    node.catalog_revision = catalog_revision;
                    node.description = snapshot.describe.clone();
                    node.archive_format = snapshot.archive_format;
                    game.upsert_live(node)?;
                }
                game.report_local_archive(
                    current_device_id.clone(),
                    snapshot.date.clone(),
                    integrity.is_some(),
                );
                Ok(())
            })
            .await?;
        Ok(())
    }

    async fn publish_current_head(
        &self,
        game_id: &str,
        local: &GameSnapshots,
    ) -> Result<(), SnapshotSyncError> {
        let Some(head) = local.head_for_device(&self.current_device_id).cloned() else {
            return Ok(());
        };
        let manifest = self.repository().load().await?;
        let Some(game) = manifest.games.get(game_id) else {
            return Ok(());
        };
        if !game.snapshots.contains_key(&head)
            || game.device_heads.get(&self.current_device_id) == Some(&head)
        {
            return Ok(());
        }
        let game_id = game_id.to_string();
        let current_device_id = self.current_device_id.clone();
        self.repository()
            .mutate(move |manifest| {
                let game = manifest
                    .games
                    .get_mut(&game_id)
                    .ok_or_else(|| ManifestError::MissingGame(game_id.clone()))?;
                if !game.snapshots.contains_key(&head) {
                    return Err(ManifestError::MissingSnapshot(head.clone()));
                }
                game.set_head(current_device_id.clone(), head.clone());
                Ok(())
            })
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

    pub(crate) fn local_path(&self, game_id: &str, snapshot: &Snapshot) -> PathBuf {
        archive_path(
            &self.local_archive_root.join(game_id),
            &snapshot.date,
            snapshot.archive_format,
        )
    }
}

fn merge_local_snapshot(
    existing: &mut SnapshotNode,
    local: &Snapshot,
    integrity: Option<ArchiveIntegrity>,
) -> Result<(), ManifestError> {
    let SnapshotState::Live(live) = &mut existing.state else {
        return Err(ManifestError::TombstoneResurrection(local.date.clone()));
    };
    if existing.parent != local.parent
        || existing.archive_format != local.archive_format
        || live.created_by != local.created_by
        || matches!(
            (&live.integrity, &integrity),
            (Some(expected), Some(actual)) if expected != actual
        )
    {
        return Err(ManifestError::SnapshotContentConflict(local.date.clone()));
    }
    if live.integrity.is_none() {
        live.integrity = integrity;
    }
    existing.description = local.describe.clone();
    Ok(())
}

/// Return missing local nodes in parent-before-child order.
///
/// The traversal starts only from post-activation nodes (those absent from the
/// captured baseline), but walks through baseline ancestors when the remote
/// graph does not know them yet. This preserves graph validity without
/// automatically uploading those inherited Archive bytes.
fn publication_order(
    manifest: &CloudManifest,
    game_id: &str,
    local: &GameSnapshots,
    local_baseline: &BTreeSet<String>,
) -> Result<Vec<Snapshot>, SnapshotSyncError> {
    let remote = manifest.games.get(game_id);
    let known: BTreeSet<String> = remote
        .map(|game| game.snapshots.keys().cloned().collect())
        .unwrap_or_default();
    let snapshots = local
        .backups
        .iter()
        .map(|snapshot| (snapshot.date.clone(), snapshot))
        .collect::<BTreeMap<_, _>>();
    let mut visiting = HashSet::new();
    let mut emitted = known.clone();
    let mut order = Vec::new();
    for snapshot in &local.backups {
        if local_baseline.contains(&snapshot.date) || known.contains(&snapshot.date) {
            continue;
        }
        visit_snapshot(
            &snapshot.date,
            &snapshots,
            &mut visiting,
            &mut emitted,
            &mut order,
        )?;
    }
    Ok(order)
}

fn visit_snapshot(
    snapshot_id: &str,
    snapshots: &BTreeMap<String, &Snapshot>,
    visiting: &mut HashSet<String>,
    emitted: &mut BTreeSet<String>,
    order: &mut Vec<Snapshot>,
) -> Result<(), SnapshotSyncError> {
    if emitted.contains(snapshot_id) {
        return Ok(());
    }
    if !visiting.insert(snapshot_id.to_string()) {
        return Err(SnapshotSyncError::LocalParentCycle(snapshot_id.to_string()));
    }
    let snapshot = snapshots
        .get(snapshot_id)
        .ok_or_else(|| SnapshotSyncError::MissingLocalAncestor(snapshot_id.to_string()))?;
    if let Some(parent) = &snapshot.parent {
        visit_snapshot(parent, snapshots, visiting, emitted, order)?;
    }
    visiting.remove(snapshot_id);
    emitted.insert(snapshot_id.to_string());
    order.push((*snapshot).clone());
    Ok(())
}

#[derive(Debug, Error)]
pub enum SnapshotSyncError {
    #[error("Local Snapshot graph is missing ancestor {0}")]
    MissingLocalAncestor(String),
    #[error("Local Snapshot parent cycle contains {0}")]
    LocalParentCycle(String),
    #[error("Snapshot Sync was cancelled")]
    Cancelled,
    #[error("Snapshot hash task failed: {0}")]
    HashTask(String),
    #[error(transparent)]
    Integrity(#[from] ArchiveIntegrityError),
    #[error(transparent)]
    Manifest(#[from] ManifestRepositoryError),
    #[error(transparent)]
    Materialization(#[from] super::MaterializationError),
    #[error(transparent)]
    Retention(#[from] SnapshotRetentionPlannerError),
    #[error(transparent)]
    Deletion(#[from] SnapshotDeletionLifecycleError),
}

#[cfg(test)]
mod tests {
    use opendal::services;

    use super::*;
    use crate::backup::{ArchiveFormat, CreatedBy};
    use crate::cloud_sync::v2::{GameManifest, SnapshotState, cloud_archive_path};

    fn memory_operator() -> Operator {
        Operator::new(services::Memory::default()).unwrap().finish()
    }

    fn snapshot(id: &str, parent: Option<&str>) -> Snapshot {
        Snapshot {
            date: id.into(),
            describe: id.into(),
            path: String::new(),
            archive_format: ArchiveFormat::Zip,
            size: 0,
            parent: parent.map(str::to_string),
            archive_hash: None,
            device_id: Some("deck".into()),
            created_by: CreatedBy::Manual,
        }
    }

    async fn write_manifest(operator: &Operator, manifest: &CloudManifest) {
        operator
            .write(
                CLOUD_MANIFEST_PATH,
                serde_json::to_vec_pretty(manifest).unwrap(),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn baseline_ancestors_are_catalogued_but_only_new_snapshots_upload() {
        let operator = memory_operator();
        let root = temp_dir::TempDir::new().unwrap();
        let archive_root = root.path().join("deck");
        let game_root = archive_root.join("game");
        std::fs::create_dir_all(&game_root).unwrap();
        std::fs::write(
            archive_path(&game_root, "baseline", ArchiveFormat::Zip),
            b"baseline",
        )
        .unwrap();
        std::fs::write(archive_path(&game_root, "new", ArchiveFormat::Zip), b"new").unwrap();
        let mut manifest = CloudManifest {
            revision: 5,
            ..Default::default()
        };
        manifest
            .games
            .insert("game".into(), GameManifest::new("game"));
        write_manifest(&operator, &manifest).await;
        let mut local = GameSnapshots::new("Game");
        local.backups = vec![
            snapshot("baseline", None),
            snapshot("new", Some("baseline")),
        ];
        local.set_head_for_device("deck".into(), Some("new".into()));
        let coordinator = SnapshotSyncCoordinator::new(
            operator.clone(),
            archive_root,
            "deck".into(),
            root.path().join("progress.json"),
            2,
        );

        let outcome = coordinator
            .reconcile_game(
                "game",
                &local,
                5,
                &BTreeSet::from(["baseline".into()]),
                &CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(
            outcome,
            SnapshotReconciliationOutcome {
                published: 2,
                uploaded: 1,
                downloaded: 0,
            }
        );
        let stored = coordinator.repository().load().await.unwrap();
        let game = &stored.games["game"];
        assert_eq!(game.snapshots["baseline"].catalog_revision, 5);
        assert!(!matches!(
            &game.snapshots["baseline"].state,
            SnapshotState::Live(live) if live.cloud_archive_verified
        ));
        assert!(matches!(
            &game.snapshots["new"].state,
            SnapshotState::Live(live) if live.cloud_archive_verified
        ));
        assert_eq!(game.device_heads["deck"], "new");
        assert!(
            operator
                .read(&cloud_archive_path("game", "baseline", ArchiveFormat::Zip).unwrap())
                .await
                .is_err()
        );
        assert!(
            operator
                .read(&cloud_archive_path("game", "new", ArchiveFormat::Zip).unwrap())
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn reconciliation_downloads_only_post_activation_cloud_archives() {
        let operator = memory_operator();
        let root = temp_dir::TempDir::new().unwrap();
        let archive_root = root.path().join("deck");
        let mut manifest = CloudManifest {
            revision: 6,
            ..Default::default()
        };
        let mut game = GameManifest::new("game");
        for (id, revision, bytes) in [("old", 5, b"old".as_slice()), ("new", 6, b"new".as_slice())]
        {
            let fixture = root.path().join(format!("{id}.zip"));
            std::fs::write(&fixture, bytes).unwrap();
            let mut node = SnapshotNode::live(
                id,
                None,
                ArchiveIntegrity::from_file(&fixture).unwrap(),
                CreatedBy::Manual,
            );
            node.catalog_revision = revision;
            let SnapshotState::Live(live) = &mut node.state else {
                unreachable!()
            };
            live.cloud_archive_verified = true;
            game.upsert_live(node).unwrap();
            operator
                .write(
                    &cloud_archive_path("game", id, ArchiveFormat::Zip).unwrap(),
                    bytes.to_vec(),
                )
                .await
                .unwrap();
        }
        manifest.games.insert("game".into(), game);
        write_manifest(&operator, &manifest).await;
        let coordinator = SnapshotSyncCoordinator::new(
            operator,
            archive_root.clone(),
            "deck".into(),
            root.path().join("progress.json"),
            2,
        );

        let outcome = coordinator
            .reconcile_game(
                "game",
                &GameSnapshots::new("Game"),
                5,
                &BTreeSet::new(),
                &CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(outcome.downloaded, 1);
        assert!(!archive_path(&archive_root.join("game"), "old", ArchiveFormat::Zip).exists());
        assert!(archive_path(&archive_root.join("game"), "new", ArchiveFormat::Zip).is_file());
    }

    #[tokio::test]
    async fn retention_enforcement_uses_global_tombstones_and_keeps_each_head() {
        let operator = memory_operator();
        let root = temp_dir::TempDir::new().unwrap();
        let archive_root = root.path().join("deck");
        let game_root = archive_root.join("game");
        std::fs::create_dir_all(&game_root).unwrap();
        let mut game = GameManifest::new("game");
        for (id, parent) in [("old", None), ("kept", Some("old")), ("head", Some("kept"))] {
            let local_path = archive_path(&game_root, id, ArchiveFormat::Zip);
            std::fs::write(&local_path, id.as_bytes()).unwrap();
            let mut node = SnapshotNode::live(
                id,
                parent.map(str::to_string),
                ArchiveIntegrity::from_file(&local_path).unwrap(),
                CreatedBy::Timer,
            );
            let SnapshotState::Live(live) = &mut node.state else {
                unreachable!()
            };
            live.cloud_archive_verified = true;
            game.upsert_live(node).unwrap();
            game.report_local_archive("deck".into(), id.into(), true);
            operator
                .write(
                    &cloud_archive_path("game", id, ArchiveFormat::Zip).unwrap(),
                    id.as_bytes().to_vec(),
                )
                .await
                .unwrap();
        }
        game.set_head("deck".into(), "head".into());
        let mut manifest = CloudManifest::default();
        manifest.games.insert("game".into(), game);
        write_manifest(&operator, &manifest).await;
        let coordinator = SnapshotSyncCoordinator::new(
            operator.clone(),
            archive_root,
            "deck".into(),
            root.path().join("progress.json"),
            2,
        );

        let outcome = coordinator.enforce_retention("game", 1).await.unwrap();

        assert_eq!(outcome.deleted, 1);
        assert_eq!(outcome.tombstones, BTreeSet::from(["old".into()]));
        let stored = coordinator.repository().load().await.unwrap();
        assert!(matches!(
            stored.games["game"].snapshots["old"].state,
            SnapshotState::FinalTombstone {
                kind: DeletionKind::Retention
            }
        ));
        assert!(stored.games["game"].snapshots["kept"].state.is_live());
        assert!(stored.games["game"].snapshots["head"].state.is_live());
        assert!(!archive_path(&game_root, "old", ArchiveFormat::Zip).exists());
        assert!(
            !operator
                .exists(&cloud_archive_path("game", "old", ArchiveFormat::Zip).unwrap())
                .await
                .unwrap()
        );
    }
}
