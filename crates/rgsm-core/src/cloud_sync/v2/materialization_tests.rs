use std::collections::BTreeMap;

use opendal::{Operator, services};
use tokio_util::sync::CancellationToken;

use super::{
    ArchiveIntegrity, CLOUD_MANIFEST_PATH, CloudArchiveMaterializer, CloudManifest, DeletionKind,
    GameManifest, LocalArchiveEviction, LocalArchiveEvidence, MaterializationError,
    SnapshotDeletionLifecycleError, SnapshotNode, SnapshotState, cloud_archive_path,
};
use crate::backup::{ArchiveFormat, CreatedBy, archive_path};

fn memory_operator() -> Operator {
    Operator::new(services::Memory::default())
        .expect("memory backend should initialize")
        .finish()
}

fn live(snapshot_id: &str, bytes: &[u8], cloud_verified: bool) -> SnapshotNode {
    let temp = temp_dir::TempDir::new().expect("temporary directory should initialize");
    let path = temp.path().join("archive.zip");
    std::fs::write(&path, bytes).expect("fixture archive should write");
    let integrity = ArchiveIntegrity::from_file(&path).expect("fixture should hash");
    let mut node = SnapshotNode::live(snapshot_id, None, integrity, CreatedBy::Manual);
    if let SnapshotState::Live(live) = &mut node.state {
        live.cloud_archive_verified = cloud_verified;
    }
    node
}

async fn write_manifest(operator: &Operator, manifest: &CloudManifest) {
    operator
        .write(
            CLOUD_MANIFEST_PATH,
            serde_json::to_vec_pretty(manifest).expect("manifest should serialize"),
        )
        .await
        .expect("manifest should write");
}

fn materializer(
    operator: Operator,
    root: &std::path::Path,
    device: &str,
) -> CloudArchiveMaterializer {
    CloudArchiveMaterializer::new(
        operator,
        root.join(device),
        device.to_string(),
        root.join(format!("{device}-materialize.json")),
        2,
    )
}

#[tokio::test]
async fn excluded_game_clears_its_old_scoped_plan_without_blocking_other_games() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().unwrap();
    let mut manifest = CloudManifest::default();
    for id in ["pending", "ready"] {
        let mut game = GameManifest::new(id);
        game.upsert_live(live("snapshot", b"bytes", true)).unwrap();
        manifest.games.insert(id.into(), game);
        operator
            .write(
                &cloud_archive_path(id, "snapshot", ArchiveFormat::Zip).unwrap(),
                b"bytes".to_vec(),
            )
            .await
            .unwrap();
    }
    write_manifest(&operator, &manifest).await;
    let original = materializer(operator.clone(), root.path(), "deck");
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        original.materialize_game("pending", 0, &cancelled).await,
        Err(MaterializationError::Cancelled)
    ));
    let connected =
        materializer(operator, root.path(), "deck").excluding_games(["pending".into()].into());
    assert!(
        connected
            .resume_pending(&CancellationToken::new())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        connected
            .materialize_game("ready", 0, &CancellationToken::new())
            .await
            .unwrap()
            .downloaded,
        1
    );
    assert!(!root.path().join("deck/pending").exists());
}

#[tokio::test]
async fn unresolved_definitions_are_excluded_from_deletion_and_batch_transfer() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().unwrap();
    let mut manifest = CloudManifest::default();
    for game_id in ["pending", "ready"] {
        let mut game = GameManifest::new(game_id);
        game.upsert_live(live("download", b"cloud bytes", true))
            .unwrap();
        let mut deleted = live("deleted", b"local bytes", false);
        deleted.state = SnapshotState::FinalTombstone {
            kind: DeletionKind::User,
        };
        game.snapshots.insert("deleted".into(), deleted);
        game.report_local_archive("pc".into(), "deleted".into(), true);
        manifest.games.insert(game_id.into(), game);
        let local = archive_path(
            &root.path().join("pc").join(game_id),
            "deleted",
            ArchiveFormat::Zip,
        );
        std::fs::create_dir_all(local.parent().unwrap()).unwrap();
        std::fs::write(local, b"local bytes").unwrap();
        operator
            .write(
                &cloud_archive_path(game_id, "download", ArchiveFormat::Zip).unwrap(),
                b"cloud bytes".to_vec(),
            )
            .await
            .unwrap();
    }
    write_manifest(&operator, &manifest).await;
    let client = materializer(operator.clone(), root.path(), "pc")
        .excluding_games(["pending".into()].into());
    let removed = client.converge_local_tombstones().await.unwrap();
    assert!(!removed.contains_key("pending"));
    assert!(removed.contains_key("ready"));
    assert_eq!(
        std::fs::read(root.path().join("pc/pending/deleted.zip")).unwrap(),
        b"local bytes"
    );
    assert_eq!(
        client
            .preview_materialize_all()
            .await
            .unwrap()
            .snapshot_count,
        1
    );
    assert_eq!(
        client
            .materialize_all(&CancellationToken::new())
            .await
            .unwrap()
            .downloaded,
        1
    );
    assert!(!root.path().join("pc/pending/download.zip").exists());
    assert_eq!(
        std::fs::read(root.path().join("pc/ready/download.zip")).unwrap(),
        b"cloud bytes"
    );
    assert!(client.download("pending", "download").await.is_err());
    assert!(
        client
            .delete_snapshot("pending", "download", true)
            .await
            .is_err()
    );
    let stored: CloudManifest =
        serde_json::from_slice(&operator.read(CLOUD_MANIFEST_PATH).await.unwrap().to_vec())
            .unwrap();
    assert_eq!(stored.games["pending"], manifest.games["pending"]);
}

#[tokio::test]
async fn resumed_transfer_rechecks_definition_scope() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().unwrap();
    let mut manifest = CloudManifest::default();
    for game_id in ["pending", "ready"] {
        let mut game = GameManifest::new(game_id);
        game.upsert_live(live("snapshot", b"bytes", true)).unwrap();
        manifest.games.insert(game_id.into(), game);
        operator
            .write(
                &cloud_archive_path(game_id, "snapshot", ArchiveFormat::Zip).unwrap(),
                b"bytes".to_vec(),
            )
            .await
            .unwrap();
    }
    write_manifest(&operator, &manifest).await;
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        materializer(operator.clone(), root.path(), "pc")
            .materialize_all(&cancelled)
            .await,
        Err(MaterializationError::Cancelled)
    ));
    let client =
        materializer(operator, root.path(), "pc").excluding_games(["pending".into()].into());
    assert_eq!(
        client
            .resume_pending(&CancellationToken::new())
            .await
            .unwrap()
            .unwrap()
            .downloaded,
        1
    );
    assert!(!root.path().join("pc/pending").exists());
    assert!(root.path().join("pc/ready/snapshot.zip").exists());
}

#[tokio::test]
async fn view_keeps_catalog_cloud_and_device_availability_separate() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().expect("temporary directory should initialize");
    let mut manifest = CloudManifest::default();
    let mut game = GameManifest::new("game");
    let local = live("local", b"local bytes", true);
    let unavailable = live("deck-only", b"deck bytes", false);
    game.upsert_live(local.clone()).unwrap();
    game.upsert_live(unavailable).unwrap();
    game.report_local_archive("pc".into(), "local".into(), true);
    game.report_local_archive("deck".into(), "deck-only".into(), true);
    manifest.games.insert("game".into(), game);
    let local_path = archive_path(
        &root.path().join("pc").join("game"),
        "local",
        ArchiveFormat::Zip,
    );
    std::fs::create_dir_all(local_path.parent().unwrap()).unwrap();
    std::fs::write(&local_path, b"local bytes").unwrap();
    write_manifest(&operator, &manifest).await;

    let view = materializer(operator, root.path(), "pc")
        .view(
            &BTreeMap::from([("game".into(), "Example".into())]),
            &BTreeMap::new(),
        )
        .await
        .unwrap();

    assert_eq!(view.games[0].local_count, 1);
    assert_eq!(view.games[0].cloud_count, 1);
    let deck_only = view.games[0]
        .snapshots
        .iter()
        .find(|snapshot| snapshot.snapshot_id == "deck-only")
        .unwrap();
    assert_eq!(deck_only.local_evidence, LocalArchiveEvidence::Mismatch);
    assert!(!deck_only.cloud_verified);
    assert_eq!(deck_only.reported_on_devices, vec!["deck"]);
    assert_eq!(deck_only.parent, None);
}

#[tokio::test]
async fn same_size_local_corruption_is_planned_for_repair() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().expect("temporary directory should initialize");
    let mut manifest = CloudManifest::default();
    let mut game = GameManifest::new("game");
    game.upsert_live(live("snapshot", b"expected", true))
        .unwrap();
    game.report_local_archive("pc".into(), "snapshot".into(), true);
    manifest.games.insert("game".into(), game);
    let local_path = archive_path(
        &root.path().join("pc").join("game"),
        "snapshot",
        ArchiveFormat::Zip,
    );
    std::fs::create_dir_all(local_path.parent().unwrap()).unwrap();
    std::fs::write(&local_path, b"corrupt!").unwrap();
    write_manifest(&operator, &manifest).await;

    let preview = materializer(operator, root.path(), "pc")
        .preview_materialize_all()
        .await
        .unwrap();

    assert_eq!(preview.snapshot_count, 1);
}

#[tokio::test]
async fn local_eviction_keeps_shared_snapshot_and_cloud_archive() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().expect("temporary directory should initialize");
    let mut manifest = CloudManifest::default();
    let mut game = GameManifest::new("game");
    game.upsert_live(live("snapshot", b"bytes", true)).unwrap();
    game.set_head("pc".into(), "snapshot".into());
    game.report_local_archive("pc".into(), "snapshot".into(), true);
    manifest.games.insert("game".into(), game);
    let local_path = archive_path(
        &root.path().join("pc").join("game"),
        "snapshot",
        ArchiveFormat::Zip,
    );
    std::fs::create_dir_all(local_path.parent().unwrap()).unwrap();
    std::fs::write(&local_path, b"bytes").unwrap();
    let cloud_path = cloud_archive_path("game", "snapshot", ArchiveFormat::Zip).unwrap();
    operator
        .write(&cloud_path, b"bytes".to_vec())
        .await
        .unwrap();
    write_manifest(&operator, &manifest).await;

    assert!(
        LocalArchiveEviction::new(operator.clone(), root.path().join("pc"), "pc".into(), 2,)
            .evict("game", "snapshot")
            .await
            .unwrap()
    );

    assert!(!local_path.exists());
    assert!(operator.exists(&cloud_path).await.unwrap());
    let stored: CloudManifest =
        serde_json::from_slice(&operator.read(CLOUD_MANIFEST_PATH).await.unwrap().to_vec())
            .unwrap();
    assert!(stored.games["game"].snapshots["snapshot"].state.is_live());
    assert_eq!(stored.games["game"].device_heads["pc"], "snapshot");
    assert!(!stored.games["game"].local_archives["pc"].contains("snapshot"));
}

#[tokio::test]
async fn another_devices_local_only_archive_cannot_be_downloaded() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().expect("temporary directory should initialize");
    let mut manifest = CloudManifest::default();
    let mut game = GameManifest::new("game");
    game.upsert_live(live("snapshot", b"bytes", false)).unwrap();
    game.report_local_archive("pc".into(), "snapshot".into(), true);
    manifest.games.insert("game".into(), game);
    write_manifest(&operator, &manifest).await;

    let error = materializer(operator, root.path(), "deck")
        .download("game", "snapshot")
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        MaterializationError::CloudArchiveUnavailable(snapshot) if snapshot == "snapshot"
    ));
}

#[tokio::test]
async fn permanent_deletion_requires_confirmation_then_removes_local_and_cloud_copies() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().expect("temporary directory should initialize");
    let mut manifest = CloudManifest::default();
    let mut game = GameManifest::new("game");
    game.upsert_live(live("snapshot", b"bytes", true)).unwrap();
    game.set_head("pc".into(), "snapshot".into());
    game.report_local_archive("pc".into(), "snapshot".into(), true);
    manifest.games.insert("game".into(), game);
    let local_path = archive_path(
        &root.path().join("pc").join("game"),
        "snapshot",
        ArchiveFormat::Zip,
    );
    std::fs::create_dir_all(local_path.parent().unwrap()).unwrap();
    std::fs::write(&local_path, b"bytes").unwrap();
    let cloud_path = cloud_archive_path("game", "snapshot", ArchiveFormat::Zip).unwrap();
    operator
        .write(&cloud_path, b"bytes".to_vec())
        .await
        .unwrap();
    write_manifest(&operator, &manifest).await;
    let pc = materializer(operator.clone(), root.path(), "pc");

    assert!(matches!(
        pc.delete_snapshot("game", "snapshot", false).await,
        Err(MaterializationError::Deletion(
            SnapshotDeletionLifecycleError::ConfirmationRequired
        ))
    ));
    assert!(local_path.is_file());
    assert!(operator.exists(&cloud_path).await.unwrap());

    pc.delete_snapshot("game", "snapshot", true).await.unwrap();

    assert!(!local_path.exists());
    assert!(!operator.exists(&cloud_path).await.unwrap());
    let stored: CloudManifest =
        serde_json::from_slice(&operator.read(CLOUD_MANIFEST_PATH).await.unwrap().to_vec())
            .unwrap();
    assert!(stored.games["game"].is_final_tombstone("snapshot"));
    assert!(stored.games["game"].device_heads.is_empty());
}

#[tokio::test]
async fn reconciling_pending_tombstone_removes_this_devices_local_copy() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().expect("temporary directory should initialize");
    let mut manifest = CloudManifest::default();
    let mut game = GameManifest::new("game");
    game.upsert_live(live("snapshot", b"bytes", true)).unwrap();
    game.report_local_archive("pc".into(), "snapshot".into(), true);
    game.begin_deletion("snapshot", "deck", DeletionKind::User)
        .unwrap();
    manifest.games.insert("game".into(), game);
    let local_path = archive_path(
        &root.path().join("pc").join("game"),
        "snapshot",
        ArchiveFormat::Zip,
    );
    std::fs::create_dir_all(local_path.parent().unwrap()).unwrap();
    std::fs::write(&local_path, b"bytes").unwrap();
    write_manifest(&operator, &manifest).await;

    let pc = materializer(operator.clone(), root.path(), "pc");
    let removed = pc.converge_local_tombstones().await.unwrap();
    assert!(removed["game"].contains("snapshot"));
    let view = pc
        .view(
            &BTreeMap::from([("game".into(), "Example".into())]),
            &BTreeMap::new(),
        )
        .await
        .unwrap();

    assert!(!local_path.exists());
    assert!(view.games[0].snapshots.is_empty());
    assert_eq!(view.games[0].pending_deletions.len(), 1);
    assert!(!view.games[0].pending_deletions[0].retryable);
    let stored: CloudManifest =
        serde_json::from_slice(&operator.read(CLOUD_MANIFEST_PATH).await.unwrap().to_vec())
            .unwrap();
    assert!(
        !stored.games["game"]
            .local_archives
            .get("pc")
            .is_some_and(|items| items.contains("snapshot"))
    );
}

#[tokio::test]
async fn initiating_device_can_directly_retry_a_pending_deletion() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().expect("temporary directory should initialize");
    let mut manifest = CloudManifest::default();
    let mut game = GameManifest::new("game");
    game.upsert_live(live("snapshot", b"bytes", true)).unwrap();
    game.begin_deletion("snapshot", "pc", DeletionKind::User)
        .unwrap();
    manifest.games.insert("game".into(), game);
    let local_path = archive_path(
        &root.path().join("pc").join("game"),
        "snapshot",
        ArchiveFormat::Zip,
    );
    std::fs::create_dir_all(local_path.parent().unwrap()).unwrap();
    std::fs::write(&local_path, b"bytes").unwrap();
    let cloud_path = cloud_archive_path("game", "snapshot", ArchiveFormat::Zip).unwrap();
    operator
        .write(&cloud_path, b"bytes".to_vec())
        .await
        .unwrap();
    write_manifest(&operator, &manifest).await;

    materializer(operator.clone(), root.path(), "pc")
        .delete_snapshot("game", "snapshot", false)
        .await
        .unwrap();

    let stored: CloudManifest =
        serde_json::from_slice(&operator.read(CLOUD_MANIFEST_PATH).await.unwrap().to_vec())
            .unwrap();
    assert!(stored.games["game"].is_final_tombstone("snapshot"));
    assert!(!local_path.exists());
    assert!(!operator.exists(&cloud_path).await.unwrap());
}

#[tokio::test]
async fn upload_and_download_publish_availability_only_after_hash_verification() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().expect("temporary directory should initialize");
    let bytes = b"verified archive";
    let mut manifest = CloudManifest::default();
    let mut game = GameManifest::new("game");
    game.upsert_live(live("snapshot", bytes, false)).unwrap();
    manifest.games.insert("game".into(), game);
    write_manifest(&operator, &manifest).await;
    let pc_path = archive_path(
        &root.path().join("pc").join("game"),
        "snapshot",
        ArchiveFormat::Zip,
    );
    std::fs::create_dir_all(pc_path.parent().unwrap()).unwrap();
    std::fs::write(&pc_path, bytes).unwrap();

    materializer(operator.clone(), root.path(), "pc")
        .upload("game", "snapshot")
        .await
        .unwrap();
    let lineage = materializer(operator.clone(), root.path(), "deck")
        .download("game", "snapshot")
        .await
        .unwrap();
    assert_eq!(lineage.len(), 1);
    assert_eq!(lineage[0].date, "snapshot");
    assert_eq!(lineage[0].parent, None);

    let deck_path = archive_path(
        &root.path().join("deck").join("game"),
        "snapshot",
        ArchiveFormat::Zip,
    );
    assert_eq!(std::fs::read(deck_path).unwrap(), bytes);
    let stored: CloudManifest =
        serde_json::from_slice(&operator.read(CLOUD_MANIFEST_PATH).await.unwrap().to_vec())
            .unwrap();
    let game = &stored.games["game"];
    let SnapshotState::Live(live) = &game.snapshots["snapshot"].state else {
        panic!("snapshot should stay live")
    };
    assert!(live.cloud_archive_verified);
    assert!(game.local_archives["pc"].contains("snapshot"));
    assert!(game.local_archives["deck"].contains("snapshot"));
}

#[tokio::test]
async fn download_returns_parent_preserving_lineage() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().expect("temporary directory should initialize");
    let parent_bytes = b"parent archive";
    let child_bytes = b"child archive";
    let mut parent = live("root", parent_bytes, true);
    parent.description = "root snapshot".into();
    let mut child = live("child", child_bytes, true);
    child.parent = Some("root".into());
    child.description = "child snapshot".into();
    let mut game = GameManifest::new("game");
    game.upsert_live(parent).unwrap();
    game.upsert_live(child).unwrap();
    let mut manifest = CloudManifest::default();
    manifest.games.insert("game".into(), game);
    write_manifest(&operator, &manifest).await;
    operator
        .write(
            &cloud_archive_path("game", "root", ArchiveFormat::Zip).unwrap(),
            parent_bytes.to_vec(),
        )
        .await
        .unwrap();
    operator
        .write(
            &cloud_archive_path("game", "child", ArchiveFormat::Zip).unwrap(),
            child_bytes.to_vec(),
        )
        .await
        .unwrap();

    let lineage = materializer(operator, root.path(), "deck")
        .download("game", "child")
        .await
        .unwrap();

    assert_eq!(
        lineage
            .iter()
            .map(|snapshot| (snapshot.date.as_str(), snapshot.parent.as_deref()))
            .collect::<Vec<_>>(),
        vec![("root", None), ("child", Some("root"))]
    );
}

#[tokio::test]
async fn materialize_all_resume_keeps_the_original_catalog_boundary() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().expect("temporary directory should initialize");
    let mut manifest = CloudManifest::default();
    let mut game = GameManifest::new("game");
    for (snapshot, bytes) in [("one", b"one".as_slice()), ("two", b"two".as_slice())] {
        game.upsert_live(live(snapshot, bytes, true)).unwrap();
        operator
            .write(
                &cloud_archive_path("game", snapshot, ArchiveFormat::Zip).unwrap(),
                bytes.to_vec(),
            )
            .await
            .unwrap();
    }
    manifest.games.insert("game".into(), game);
    write_manifest(&operator, &manifest).await;
    let deck = materializer(operator.clone(), root.path(), "deck");
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        deck.materialize_all(&cancelled).await,
        Err(MaterializationError::Cancelled)
    ));

    let mut changed: CloudManifest =
        serde_json::from_slice(&operator.read(CLOUD_MANIFEST_PATH).await.unwrap().to_vec())
            .unwrap();
    changed
        .games
        .get_mut("game")
        .unwrap()
        .upsert_live(live("later", b"later", true))
        .unwrap();
    operator
        .write(
            &cloud_archive_path("game", "later", ArchiveFormat::Zip).unwrap(),
            b"later".to_vec(),
        )
        .await
        .unwrap();
    write_manifest(&operator, &changed).await;

    let outcome = deck
        .materialize_all(&CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(outcome.downloaded, 2);
    assert!(
        !archive_path(
            &root.path().join("deck").join("game"),
            "later",
            ArchiveFormat::Zip
        )
        .exists()
    );
}

#[tokio::test]
async fn game_materialization_respects_scope_and_activation_revision() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().expect("temporary directory should initialize");
    let mut manifest = CloudManifest {
        revision: 4,
        ..Default::default()
    };
    for (game_id, snapshots) in [
        ("selected", [("before", 3), ("after", 4)]),
        ("other", [("other", 1), ("other-later", 2)]),
    ] {
        let mut game = GameManifest::new(game_id);
        for (snapshot_id, catalog_revision) in snapshots {
            let bytes = snapshot_id.as_bytes();
            let mut node = live(snapshot_id, bytes, true);
            node.catalog_revision = catalog_revision;
            game.upsert_live(node).unwrap();
            operator
                .write(
                    &cloud_archive_path(game_id, snapshot_id, ArchiveFormat::Zip).unwrap(),
                    bytes.to_vec(),
                )
                .await
                .unwrap();
        }
        manifest.games.insert(game_id.into(), game);
    }
    write_manifest(&operator, &manifest).await;

    let outcome = materializer(operator.clone(), root.path(), "deck")
        .materialize_game("selected", 3, &CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(outcome.downloaded, 1);
    assert!(
        archive_path(
            &root.path().join("deck").join("selected"),
            "before",
            ArchiveFormat::Zip
        )
        .is_file()
    );
    assert!(
        !archive_path(
            &root.path().join("deck").join("selected"),
            "after",
            ArchiveFormat::Zip
        )
        .exists()
    );
    assert!(
        !archive_path(
            &root.path().join("deck").join("other"),
            "other",
            ArchiveFormat::Zip
        )
        .exists()
    );

    let post_activation = materializer(operator, root.path(), "laptop")
        .materialize_game_since("selected", 3, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(post_activation.downloaded, 1);
    assert!(
        !archive_path(
            &root.path().join("laptop").join("selected"),
            "before",
            ArchiveFormat::Zip
        )
        .exists()
    );
    assert!(
        archive_path(
            &root.path().join("laptop").join("selected"),
            "after",
            ArchiveFormat::Zip
        )
        .is_file()
    );
}

#[tokio::test]
async fn catch_up_uses_current_catalog_revision_not_legacy_zero() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().expect("temporary directory should initialize");
    let mut manifest = CloudManifest {
        revision: 4,
        ..Default::default()
    };
    let mut game = GameManifest::new("selected");
    let bytes = b"published after join";
    let mut node = live("later", bytes, true);
    node.catalog_revision = 4;
    game.upsert_live(node).unwrap();
    operator
        .write(
            &cloud_archive_path("selected", "later", ArchiveFormat::Zip).unwrap(),
            bytes.to_vec(),
        )
        .await
        .unwrap();
    manifest.games.insert("selected".into(), game);
    write_manifest(&operator, &manifest).await;

    let deck = materializer(operator.clone(), root.path(), "deck");
    assert_eq!(
        deck.materialize_game("selected", 0, &CancellationToken::new())
            .await
            .unwrap()
            .downloaded,
        0
    );
    let current = deck.catalog_revision().await.unwrap();
    assert_eq!(current, 4);
    assert_eq!(
        deck.materialize_game("selected", current, &CancellationToken::new())
            .await
            .unwrap()
            .downloaded,
        1
    );
    assert!(
        archive_path(
            &root.path().join("deck").join("selected"),
            "later",
            ArchiveFormat::Zip
        )
        .is_file()
    );
}

#[tokio::test]
async fn materialize_all_resumes_the_one_pending_scope() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().expect("temporary directory should initialize");
    let mut manifest = CloudManifest::default();
    let mut game = GameManifest::new("game");
    game.upsert_live(live("snapshot", b"bytes", true)).unwrap();
    manifest.games.insert("game".into(), game);
    operator
        .write(
            &cloud_archive_path("game", "snapshot", ArchiveFormat::Zip).unwrap(),
            b"bytes".to_vec(),
        )
        .await
        .unwrap();
    write_manifest(&operator, &manifest).await;
    let deck = materializer(operator.clone(), root.path(), "deck");
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        deck.materialize_game("game", 0, &cancelled).await,
        Err(MaterializationError::Cancelled)
    ));

    assert!(matches!(
        deck.preview_game("other", 0).await,
        Err(MaterializationError::AnotherMaterializationPending)
    ));
    assert_eq!(
        deck.preview_materialize_all().await.unwrap().snapshot_count,
        1
    );
    assert_eq!(
        deck.materialize_all(&CancellationToken::new())
            .await
            .unwrap()
            .downloaded,
        1
    );
}

#[tokio::test]
async fn pending_plan_is_replanned_when_its_snapshot_identity_changes_remotely() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().expect("temporary directory should initialize");
    let mut manifest = CloudManifest::default();
    let mut game = GameManifest::new("game");
    game.upsert_live(live("snapshot", b"bytes", true)).unwrap();
    manifest.games.insert("game".into(), game);
    operator
        .write(
            &cloud_archive_path("game", "snapshot", ArchiveFormat::Zip).unwrap(),
            b"bytes".to_vec(),
        )
        .await
        .unwrap();
    write_manifest(&operator, &manifest).await;
    let deck = materializer(operator.clone(), root.path(), "deck");
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        deck.materialize_all(&cancelled).await,
        Err(MaterializationError::Cancelled)
    ));

    manifest
        .games
        .get_mut("game")
        .unwrap()
        .snapshots
        .insert("snapshot".into(), live("snapshot", b"replacement", true));
    operator
        .write(
            &cloud_archive_path("game", "snapshot", ArchiveFormat::Zip).unwrap(),
            b"replacement".to_vec(),
        )
        .await
        .unwrap();
    write_manifest(&operator, &manifest).await;

    let resumed = deck
        .resume_pending(&CancellationToken::new())
        .await
        .expect("stale progress should be replanned")
        .expect("the pending operation should finish");
    assert_eq!(resumed.downloaded, 1);
    assert!(!root.path().join("deck-materialize.json").exists());
}

#[tokio::test]
async fn pending_plan_skips_a_snapshot_deleted_remotely() {
    let operator = memory_operator();
    let root = temp_dir::TempDir::new().expect("temporary directory should initialize");
    let mut manifest = CloudManifest::default();
    let mut game = GameManifest::new("game");
    game.upsert_live(live("snapshot", b"bytes", true)).unwrap();
    manifest.games.insert("game".into(), game);
    operator
        .write(
            &cloud_archive_path("game", "snapshot", ArchiveFormat::Zip).unwrap(),
            b"bytes".to_vec(),
        )
        .await
        .unwrap();
    write_manifest(&operator, &manifest).await;
    let deck = materializer(operator.clone(), root.path(), "deck");
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        deck.materialize_all(&cancelled).await,
        Err(MaterializationError::Cancelled)
    ));

    manifest
        .games
        .get_mut("game")
        .unwrap()
        .snapshots
        .get_mut("snapshot")
        .unwrap()
        .state = SnapshotState::FinalTombstone {
        kind: DeletionKind::User,
    };
    write_manifest(&operator, &manifest).await;

    let resumed = deck
        .resume_pending(&CancellationToken::new())
        .await
        .expect("deleted pending items should be skipped");
    assert!(resumed.is_none());
    assert!(!root.path().join("deck-materialize.json").exists());
}
