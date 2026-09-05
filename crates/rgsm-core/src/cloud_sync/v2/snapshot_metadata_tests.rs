use std::collections::BTreeMap;

use opendal::{Operator, services};

use super::*;
use crate::backup::{GameSnapshots, Snapshot};

const SNAPSHOT_ID: &str = "2026-08-01_12-00-00";
const CREATED_AT: i64 = 1_785_585_600_123;

#[tokio::test]
async fn transfer_preserves_original_creator_and_time_without_changing_archive_identity() {
    let operator = Operator::new(services::Memory::default()).unwrap().finish();
    let root = temp_dir::TempDir::new().unwrap();
    let game_root = root.path().join("uploader/game");
    std::fs::create_dir_all(&game_root).unwrap();
    let path = game_root.join(format!("{SNAPSHOT_ID}.zip"));
    std::fs::write(&path, b"unchanged legacy archive").unwrap();
    let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
        "date": SNAPSHOT_ID, "describe": "Legacy progress", "path": path,
        "device_id": "original-creator", "created_at": CREATED_AT,
    }))
    .unwrap();
    let mut local = GameSnapshots::new("Game");
    local.backups.push(snapshot.clone());
    local.set_head_for_device("uploader".into(), Some(SNAPSHOT_ID.into()));
    let uploader = SnapshotSyncCoordinator::new(
        operator.clone(),
        root.path().join("uploader"),
        "uploader".into(),
        root.path().join("upload.json"),
        2,
    );
    uploader
        .upload_local_snapshot("game", &snapshot, &local)
        .await
        .unwrap();

    let receiver = CloudArchiveMaterializer::new(
        operator.clone(),
        root.path().join("receiver"),
        "receiver".into(),
        root.path().join("download.json"),
        2,
    );
    receiver.download("game", SNAPSHOT_ID).await.unwrap();
    let repository = CloudManifestRepository::new(operator.clone(), CLOUD_MANIFEST_PATH, 2);
    let manifest = repository.load().await.unwrap();
    let imported = receiver
        .imported_lineage(&manifest, "game", SNAPSHOT_ID)
        .unwrap();
    assert_eq!(imported[0].device_id.as_deref(), Some("original-creator"));
    assert_eq!(
        serde_json::to_value(&imported[0]).unwrap()["created_at"],
        CREATED_AT
    );
    assert_eq!(imported[0].date, SNAPSHOT_ID);
    assert_eq!(
        std::fs::read(&imported[0].path).unwrap(),
        b"unchanged legacy archive"
    );
    let view = serde_json::to_value(
        receiver
            .view(&BTreeMap::new(), &BTreeMap::new())
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        view["games"][0]["snapshots"][0]["device_id"],
        "original-creator"
    );
    assert_eq!(view["games"][0]["snapshots"][0]["created_at"], CREATED_AT);

    // The explicit remote-progress path must preserve the same metadata too.
    let resolver = V2RemoteProgressResolver::new(
        operator.clone(),
        root.path().join("third"),
        "third".into(),
        root.path().join("third.json"),
        2,
    );
    let prepared = resolver
        .prepare(
            "game",
            manifest.revision,
            None,
            SNAPSHOT_ID,
            &GameSnapshots::new("Game"),
        )
        .await
        .unwrap();
    assert_eq!(
        prepared.lineage[0].device_id.as_deref(),
        Some("original-creator")
    );
    assert_eq!(
        serde_json::to_value(&prepared.lineage[0]).unwrap()["created_at"],
        CREATED_AT
    );

    // Re-publication from another holder is not creation of a new Snapshot.
    let relay = SnapshotSyncCoordinator::new(
        operator,
        root.path().join("receiver"),
        "receiver".into(),
        root.path().join("relay.json"),
        2,
    );
    let mut received = GameSnapshots::new("Game");
    received.backups = imported;
    relay
        .upload_local_snapshot("game", &received.backups[0], &received)
        .await
        .unwrap();
    let published = serde_json::to_value(repository.load().await.unwrap()).unwrap();
    assert_eq!(
        published["games"]["game"]["snapshots"][SNAPSHOT_ID]["device_id"],
        "original-creator"
    );
    assert_eq!(
        published["games"]["game"]["snapshots"][SNAPSHOT_ID]["created_at"],
        CREATED_AT
    );
}

#[test]
fn optional_creation_time_round_trips_without_rewriting_legacy_ids() {
    let mut raw = serde_json::json!({"date": SNAPSHOT_ID, "describe": "", "path": "legacy.zip"});
    let legacy: Snapshot = serde_json::from_value(raw.clone()).unwrap();
    let stored = serde_json::to_value(legacy).unwrap();
    assert_eq!(stored["date"], SNAPSHOT_ID);
    assert!(stored.get("created_at").is_none());
    assert!(stored.get("device_id").is_none());
    raw["created_at"] = CREATED_AT.into();
    let timed: Snapshot = serde_json::from_value(raw).unwrap();
    assert_eq!(
        serde_json::to_value(timed).unwrap()["created_at"],
        CREATED_AT
    );
}

#[tokio::test]
async fn legacy_cloud_metadata_can_be_filled_but_transfers_never_relabel_known_origin() {
    let operator = Operator::new(services::Memory::default()).unwrap().finish();
    let root = temp_dir::TempDir::new().unwrap();
    let game_root = root.path().join("game");
    std::fs::create_dir_all(&game_root).unwrap();
    let path = game_root.join(format!("{SNAPSHOT_ID}.zip"));
    std::fs::write(&path, b"old bytes").unwrap();
    let mut original: Snapshot = serde_json::from_value(serde_json::json!({
        "date": SNAPSHOT_ID, "describe": "", "path": path,
    }))
    .unwrap();
    let mut history = GameSnapshots::new("Game");
    history.backups.push(original.clone());
    let coordinator = SnapshotSyncCoordinator::new(
        operator.clone(),
        root.path().into(),
        "holder".into(),
        root.path().join("progress.json"),
        2,
    );
    coordinator
        .upload_local_snapshot("game", &original, &history)
        .await
        .unwrap();
    let repository = CloudManifestRepository::new(operator, CLOUD_MANIFEST_PATH, 2);
    let old = repository.load().await.unwrap();
    let imported = coordinator
        .materializer()
        .imported_lineage(&old, "game", SNAPSHOT_ID)
        .unwrap();
    assert_eq!(imported[0].device_id, None);
    assert_eq!(imported[0].created_at, None);

    original.device_id = Some("original-creator".into());
    original.created_at = Some(CREATED_AT);
    coordinator
        .upload_local_snapshot("game", &original, &history)
        .await
        .unwrap();
    let recorded = repository.load().await.unwrap().games["game"].snapshots[SNAPSHOT_ID].clone();
    assert_eq!(recorded.device_id, original.device_id);
    assert_eq!(recorded.created_at, original.created_at);
    original.device_id = Some("different-holder".into());
    original.created_at = Some(CREATED_AT + 1000);
    original.describe = "Stale holder description".into();
    coordinator
        .upload_local_snapshot("game", &original, &history)
        .await
        .unwrap();
    assert_eq!(
        repository.load().await.unwrap().games["game"].snapshots[SNAPSHOT_ID],
        recorded
    );
}
