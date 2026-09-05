use opendal::{Operator, services};

use super::*;
use crate::backup::{ArchiveFormat, CreatedBy, GameSnapshots};

async fn fixture() -> (Operator, temp_dir::TempDir, CloudManifest) {
    let operator = Operator::new(services::Memory::default()).unwrap().finish();
    let root = temp_dir::TempDir::new().unwrap();
    let source = root.path().join("source.zip");
    std::fs::write(&source, b"live descendant").unwrap();
    let integrity = ArchiveIntegrity::from_file(&source).unwrap();
    let mut game = GameManifest::new("game");
    for (id, parent) in [
        ("root", None),
        ("deleted-a", Some("root")),
        ("deleted-b", Some("deleted-a")),
        ("leaf", Some("deleted-b")),
    ] {
        let mut node = SnapshotNode::live(
            id,
            parent.map(str::to_owned),
            integrity.clone(),
            CreatedBy::Manual,
        );
        if let SnapshotState::Live(live) = &mut node.state {
            live.cloud_archive_verified = true;
        }
        game.upsert_live(node).unwrap();
    }
    game.snapshots.get_mut("deleted-a").unwrap().state = SnapshotState::FinalTombstone {
        kind: DeletionKind::User,
    };
    game.snapshots.get_mut("deleted-b").unwrap().state =
        SnapshotState::PendingTombstone(PendingTombstone {
            kind: DeletionKind::Retention,
            acting_device: "source".into(),
            acting_local_removed: false,
            cloud_archive_absent: true,
        });
    game.set_head("source".into(), "leaf".into());
    let manifest = CloudManifest {
        games: [("game".into(), game)].into(),
        ..Default::default()
    };
    operator
        .write(CLOUD_MANIFEST_PATH, serde_json::to_vec(&manifest).unwrap())
        .await
        .unwrap();
    operator
        .write(
            &cloud_archive_path("game", "leaf", ArchiveFormat::Zip).unwrap(),
            b"live descendant".to_vec(),
        )
        .await
        .unwrap();
    (operator, root, manifest)
}

#[tokio::test]
async fn deleted_ancestor_download_and_reupload_preserve_identity_without_resurrection() {
    let (operator, root, _) = fixture().await;
    let materializer = CloudArchiveMaterializer::new(
        operator.clone(),
        root.path().join("receiver"),
        "receiver".into(),
        root.path().join("download.json"),
        2,
    );
    let imported = materializer.download("game", "leaf").await.unwrap();
    assert_eq!(
        imported
            .iter()
            .map(|s| (s.date.as_str(), s.parent.as_deref()))
            .collect::<Vec<_>>(),
        vec![("root", None), ("leaf", Some("deleted-b"))]
    );
    assert_eq!(
        std::fs::read(&imported[1].path).unwrap(),
        b"live descendant"
    );
    assert!(!root.path().join("receiver/game/deleted-b.zip").exists());
    let mut local = GameSnapshots::new("Game");
    local.backups = imported;
    local.set_head_for_device("receiver".into(), Some("leaf".into()));
    let coordinator = SnapshotSyncCoordinator::new(
        operator.clone(),
        root.path().join("receiver"),
        "receiver".into(),
        root.path().join("sync.json"),
        2,
    );
    coordinator
        .upload_local_snapshot("game", &local.backups[1], &local)
        .await
        .unwrap();
    let manifest = CloudManifestRepository::new(operator, CLOUD_MANIFEST_PATH, 2)
        .load()
        .await
        .unwrap();
    let game = &manifest.games["game"];
    assert_eq!(game.snapshots.len(), 4);
    assert!(!game.snapshots["deleted-a"].state.is_live());
    assert!(!game.snapshots["deleted-b"].state.is_live());
    assert_eq!(game.snapshots["leaf"].parent.as_deref(), Some("deleted-b"));
    assert!(game.is_ancestor_or_equal("root", "leaf").unwrap());
    game.validate().unwrap();
}

#[tokio::test]
async fn deleted_ancestor_does_not_block_explicit_remote_progress() {
    let (operator, root, manifest) = fixture().await;
    let resolver = V2RemoteProgressResolver::new(
        operator,
        root.path().join("receiver"),
        "receiver".into(),
        root.path().join("progress.json"),
        2,
    );
    let prepared = resolver
        .prepare(
            "game",
            manifest.revision,
            None,
            "leaf",
            &GameSnapshots::new("Game"),
        )
        .await
        .unwrap();
    assert_eq!(
        prepared
            .lineage
            .iter()
            .map(|s| s.date.as_str())
            .collect::<Vec<_>>(),
        vec!["root", "leaf"]
    );
    assert_eq!(prepared.lineage[1].parent.as_deref(), Some("deleted-b"));
    assert_eq!(
        std::fs::read(&prepared.lineage[1].path).unwrap(),
        b"live descendant"
    );
}

#[tokio::test]
async fn lineage_retains_live_metadata_without_an_archive_and_rejects_invalid_chains() {
    let (_, root, manifest) = fixture().await;
    let mut game = manifest.games["game"].clone();
    game.snapshots.insert(
        "root".into(),
        SnapshotNode::unavailable("root", None, CreatedBy::Manual),
    );
    let imported = game.local_lineage("leaf", root.path()).unwrap();
    assert_eq!(imported[0].date, "root");
    assert_eq!(imported[0].archive_hash, None);
    assert!(matches!(
        game.local_lineage("deleted-a", root.path()),
        Err(ManifestError::ExpectedLive(_))
    ));
    game.snapshots.get_mut("deleted-a").unwrap().parent = Some("leaf".into());
    assert!(matches!(
        game.local_lineage("leaf", root.path()),
        Err(ManifestError::ParentCycle(_))
    ));
    game.snapshots.get_mut("deleted-a").unwrap().parent = Some("missing".into());
    assert!(matches!(
        game.local_lineage("leaf", root.path()),
        Err(ManifestError::MissingSnapshot(_))
    ));
    assert!(matches!(
        game.local_lineage("missing", root.path()),
        Err(ManifestError::MissingSnapshot(_))
    ));
}
