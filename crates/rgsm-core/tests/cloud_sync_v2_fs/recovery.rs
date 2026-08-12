use rgsm_core::cloud_sync::v2::{
    CLOUD_MANIFEST_PATH, CloudArchiveMaterializer, CloudManifestRepository,
    DeletionRegistryRepository, DeviceProfileRepository, SharedGameDeletion,
    SharedLibraryRepository,
};
use tokio_util::sync::CancellationToken;

use crate::support::{
    DeviceFixture, FsCloudFixture, MAX_ATTEMPTS, bootstrap_game, cloud_archive, read_progress,
    reconcile_game, snapshot,
};

const GAME_ID: &str = "example-game";
const GAME_NAME: &str = "Example Game";
const SNAPSHOT_ID: &str = "snapshot-a";
const ARCHIVE_BYTES: &[u8] = b"recovery archive bytes";

async fn populated_game() -> (
    FsCloudFixture,
    DeviceFixture,
    DeviceFixture,
    rgsm_core::backup::Snapshot,
) {
    let cloud = FsCloudFixture::new();
    let device_a = DeviceFixture::new("device-a");
    let device_b = DeviceFixture::new("device-b");
    bootstrap_game(&cloud, &device_a, Some(&device_b), GAME_ID, GAME_NAME).await;

    let snapshot = snapshot(SNAPSHOT_ID, None, &device_a.id, ARCHIVE_BYTES.len());
    device_a.write_archive(GAME_ID, &snapshot, ARCHIVE_BYTES);
    let snapshots = device_a.snapshots(GAME_NAME, vec![snapshot.clone()], SNAPSHOT_ID);
    reconcile_game(&cloud, &device_a, GAME_ID, &snapshots).await;
    (cloud, device_a, device_b, snapshot)
}

async fn assert_deleted(
    cloud: &FsCloudFixture,
    device: &DeviceFixture,
    snapshot: &rgsm_core::backup::Snapshot,
) {
    assert!(
        DeletionRegistryRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
            .load()
            .await
            .expect("Deletion Registry should reload")
            .deleted_games
            .contains_key(GAME_ID)
    );
    assert!(!device.archive_root.join(GAME_ID).exists());
    assert!(
        !cloud
            .new_operator()
            .exists(&cloud_archive(GAME_ID, snapshot))
            .await
            .expect("cloud archive absence should be observable")
    );
    assert!(
        SharedLibraryRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
            .load()
            .await
            .expect("Shared Library should reload")
            .games
            .iter()
            .all(|game| game.storage_key != GAME_ID)
    );
    assert!(
        DeviceProfileRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
            .list()
            .await
            .expect("Device Profiles should reload")
            .iter()
            .all(|profile| !profile.games.contains_key(GAME_ID))
    );
    assert!(
        !CloudManifestRepository::new(cloud.new_operator(), CLOUD_MANIFEST_PATH, MAX_ATTEMPTS)
            .load()
            .await
            .expect("Cloud Manifest should reload")
            .games
            .contains_key(GAME_ID)
    );
}

async fn retry_deletion_after(
    cloud: &FsCloudFixture,
    device: &DeviceFixture,
    snapshot: &rgsm_core::backup::Snapshot,
) {
    SharedGameDeletion::new(
        cloud.new_operator(),
        device.archive_root.clone(),
        device.id.clone(),
        MAX_ATTEMPTS,
    )
    .delete(GAME_ID, GAME_NAME, true)
    .await
    .expect("retry should converge the durable partial deletion state");
    assert_deleted(cloud, device, snapshot).await;
}

#[tokio::test]
async fn materialize_all_stops_at_damaged_archive_and_resumes_after_repair() {
    let cloud = FsCloudFixture::new();
    let source = DeviceFixture::new("device-a");
    let receiver = DeviceFixture::new("device-b");
    bootstrap_game(&cloud, &source, Some(&receiver), GAME_ID, GAME_NAME).await;

    let first = snapshot("snapshot-1", None, &source.id, b"first bytes".len());
    let middle = snapshot(
        "snapshot-2",
        Some("snapshot-1"),
        &source.id,
        b"middle bytes".len(),
    );
    let last = snapshot(
        "snapshot-3",
        Some("snapshot-2"),
        &source.id,
        b"last bytes".len(),
    );
    source.write_archive(GAME_ID, &first, b"first bytes");
    source.write_archive(GAME_ID, &middle, b"middle bytes");
    source.write_archive(GAME_ID, &last, b"last bytes");
    let snapshots = source.snapshots(
        GAME_NAME,
        vec![first.clone(), middle.clone(), last.clone()],
        "snapshot-3",
    );
    reconcile_game(&cloud, &source, GAME_ID, &snapshots).await;

    cloud
        .new_operator()
        .write(&cloud_archive(GAME_ID, &middle), b"x".to_vec())
        .await
        .expect("middle Fs archive should be damageable");
    let materializer = CloudArchiveMaterializer::new(
        cloud.new_operator(),
        receiver.archive_root.clone(),
        receiver.id.clone(),
        receiver.progress_path.clone(),
        MAX_ATTEMPTS,
    );
    assert!(
        materializer
            .materialize_all(&CancellationToken::new())
            .await
            .is_err()
    );

    assert_eq!(
        std::fs::read(receiver.archive_path(GAME_ID, &first))
            .expect("first archive should remain committed"),
        b"first bytes"
    );
    assert!(!receiver.archive_path(GAME_ID, &middle).exists());
    assert!(!receiver.archive_path(GAME_ID, &last).exists());
    let progress = read_progress(&receiver);
    assert_eq!(progress["remaining"].as_array().map(Vec::len), Some(2));
    assert_eq!(
        progress["remaining"][0]["snapshot_id"].as_str(),
        Some("snapshot-2")
    );

    cloud
        .new_operator()
        .write(&cloud_archive(GAME_ID, &middle), b"middle bytes".to_vec())
        .await
        .expect("middle Fs archive should be repairable");
    let resumed = CloudArchiveMaterializer::new(
        cloud.new_operator(),
        receiver.archive_root.clone(),
        receiver.id.clone(),
        receiver.progress_path.clone(),
        MAX_ATTEMPTS,
    )
    .resume_pending(&CancellationToken::new())
    .await
    .expect("fresh materializer should resume the durable plan")
    .expect("damaged batch should leave a pending plan");
    assert_eq!(resumed.downloaded, 2);
    assert_eq!(
        std::fs::read(receiver.archive_path(GAME_ID, &middle))
            .expect("repaired middle archive should materialize"),
        b"middle bytes"
    );
    assert_eq!(
        std::fs::read(receiver.archive_path(GAME_ID, &last))
            .expect("later archive should materialize after repair"),
        b"last bytes"
    );
    assert!(!receiver.progress_path.exists());
}

#[tokio::test]
async fn game_deletion_recovers_after_marker_written() {
    let (cloud, device_a, _, snapshot) = populated_game().await;
    DeletionRegistryRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
        .mark_game_deleted(GAME_ID, GAME_NAME, &device_a.id)
        .await
        .expect("durable deletion marker should persist");

    retry_deletion_after(&cloud, &device_a, &snapshot).await;
}

#[tokio::test]
async fn game_deletion_recovers_after_local_archives_removed() {
    let (cloud, device_a, _, snapshot) = populated_game().await;
    DeletionRegistryRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
        .mark_game_deleted(GAME_ID, GAME_NAME, &device_a.id)
        .await
        .expect("durable deletion marker should persist");
    std::fs::remove_dir_all(device_a.archive_root.join(GAME_ID))
        .expect("local game archive directory should be removable");

    retry_deletion_after(&cloud, &device_a, &snapshot).await;
}

#[tokio::test]
async fn game_deletion_recovers_after_cloud_archives_removed() {
    let (cloud, device_a, _, snapshot) = populated_game().await;
    DeletionRegistryRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
        .mark_game_deleted(GAME_ID, GAME_NAME, &device_a.id)
        .await
        .expect("durable deletion marker should persist");
    std::fs::remove_dir_all(device_a.archive_root.join(GAME_ID))
        .expect("local game archive directory should be removable");
    cloud
        .new_operator()
        .delete_with(&format!("v2/archives/{GAME_ID}/"))
        .recursive(true)
        .await
        .expect("cloud archive prefix should be removable through the Fs operator");

    retry_deletion_after(&cloud, &device_a, &snapshot).await;
}

#[tokio::test]
async fn game_deletion_recovers_after_shared_metadata_cleanup_begins() {
    let (cloud, device_a, _, snapshot) = populated_game().await;
    DeletionRegistryRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
        .mark_game_deleted(GAME_ID, GAME_NAME, &device_a.id)
        .await
        .expect("durable deletion marker should persist");
    std::fs::remove_dir_all(device_a.archive_root.join(GAME_ID))
        .expect("local game archive directory should be removable");
    cloud
        .new_operator()
        .delete_with(&format!("v2/archives/{GAME_ID}/"))
        .recursive(true)
        .await
        .expect("cloud archive prefix should be removable through the Fs operator");
    let repository = SharedLibraryRepository::new(cloud.new_operator(), MAX_ATTEMPTS);
    let expected = repository.load().await.expect("Shared Library should load");
    let mut accepted = expected.clone();
    accepted.games.retain(|game| game.storage_key != GAME_ID);
    repository
        .compare_replace(&expected, &accepted)
        .await
        .expect("shared metadata cleanup should persist before interruption");

    retry_deletion_after(&cloud, &device_a, &snapshot).await;
}
