use std::collections::BTreeMap;

use opendal::{Operator, services};
use tokio_util::sync::CancellationToken;

use super::{
    ArchiveIntegrity, CLOUD_MANIFEST_PATH, CloudArchiveMaterializer, CloudManifest, GameManifest,
    MaterializationError, SnapshotNode, SnapshotState, cloud_archive_path,
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
        .view(&BTreeMap::from([("game".into(), "Example".into())]))
        .await
        .unwrap();

    assert_eq!(view.games[0].local_count, 1);
    assert_eq!(view.games[0].cloud_count, 1);
    let deck_only = view.games[0]
        .snapshots
        .iter()
        .find(|snapshot| snapshot.snapshot_id == "deck-only")
        .unwrap();
    assert!(!deck_only.local_verified);
    assert!(!deck_only.cloud_verified);
    assert_eq!(deck_only.reported_on_devices, vec!["deck"]);
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
    materializer(operator.clone(), root.path(), "deck")
        .download("game", "snapshot")
        .await
        .unwrap();

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
    let deck = materializer(operator, root.path(), "deck");
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
