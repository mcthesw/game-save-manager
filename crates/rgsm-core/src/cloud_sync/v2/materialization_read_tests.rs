use std::collections::BTreeMap;

use opendal::{Operator, services};

use super::{
    ArchiveIntegrity, CLOUD_MANIFEST_PATH, CloudArchiveMaterializer, CloudManifest, DeletionKind,
    GameManifest, SnapshotNode, SnapshotState,
};
use crate::backup::{ArchiveFormat, CreatedBy, archive_path};

async fn assert_read_does_not_reconcile(preview: bool) {
    let root = temp_dir::TempDir::new().unwrap();
    let operator = Operator::new(services::Memory::default()).unwrap().finish();
    let local = archive_path(&root.path().join("game"), "deleted", ArchiveFormat::Zip);
    std::fs::create_dir_all(local.parent().unwrap()).unwrap();
    std::fs::write(&local, b"archive awaiting explicit reconciliation").unwrap();
    let mut node = SnapshotNode::live(
        "deleted",
        None,
        ArchiveIntegrity::from_file(&local).unwrap(),
        CreatedBy::Manual,
    );
    node.state = SnapshotState::FinalTombstone {
        kind: DeletionKind::User,
    };
    let mut game = GameManifest::new("game");
    game.snapshots.insert("deleted".into(), node);
    game.report_local_archive("pc".into(), "deleted".into(), true);
    let mut manifest = CloudManifest::default();
    manifest.games.insert("game".into(), game);
    let original = serde_json::to_vec_pretty(&manifest).unwrap();
    operator
        .write(CLOUD_MANIFEST_PATH, original.clone())
        .await
        .unwrap();
    let progress = root.path().join("materialize.json");
    let materializer = CloudArchiveMaterializer::new(
        operator.clone(),
        root.path().to_path_buf(),
        "pc".into(),
        progress.clone(),
        2,
    );

    if preview {
        assert_eq!(
            materializer
                .preview_materialize_all()
                .await
                .unwrap()
                .snapshot_count,
            0
        );
    } else {
        let view = materializer
            .view(&BTreeMap::new(), &BTreeMap::new())
            .await
            .unwrap();
        assert!(view.games[0].snapshots.is_empty());
    }
    assert!(
        local.is_file(),
        "a read must not delete local archive bytes"
    );
    assert_eq!(
        operator.read(CLOUD_MANIFEST_PATH).await.unwrap().to_vec(),
        original
    );
    assert!(
        !progress.exists(),
        "a preview must not persist a download plan"
    );

    // The explicit reconciliation still performs the necessary cleanup.
    let removed = materializer.converge_local_tombstones().await.unwrap();
    assert!(removed["game"].contains("deleted"));
    assert!(!local.exists());
}

#[tokio::test]
async fn catalog_view_does_not_delete_archives_or_publish_presence() {
    assert_read_does_not_reconcile(false).await;
}

#[tokio::test]
async fn download_preview_does_not_delete_archives_or_publish_presence() {
    assert_read_does_not_reconcile(true).await;
}
