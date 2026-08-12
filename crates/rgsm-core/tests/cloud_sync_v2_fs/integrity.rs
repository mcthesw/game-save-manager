use opendal::ErrorKind;
use rgsm_core::cloud_sync::v2::{
    ArchiveIntegrityError, CLOUD_MANIFEST_PATH, CloudArchiveMaterializer, CloudManifest,
    CloudManifestRepository, DeletionRegistryRepository, DeviceProfileRepository,
    MaterializationError, SharedGameDeletion, SharedLibraryRepository, SnapshotSyncCoordinator,
};
use rgsm_core::preclude::BackendError;
use tokio_util::sync::CancellationToken;

use crate::support::{
    DeviceFixture, FsCloudFixture, MAX_ATTEMPTS, bootstrap_game, cloud_archive, load_manifest,
    reconcile_game, snapshot,
};

const GAME_ID: &str = "example-game";
const GAME_NAME: &str = "Example Game";
const SNAPSHOT_ID: &str = "snapshot-a";
const ARCHIVE_BYTES: &[u8] = b"verified cloud archive";
const EXISTING_BYTES: &[u8] = b"preexisting receiver archive";

async fn published_snapshot() -> (
    FsCloudFixture,
    DeviceFixture,
    DeviceFixture,
    rgsm_core::backup::Snapshot,
) {
    let cloud = FsCloudFixture::new();
    let source = DeviceFixture::new("device-a");
    let receiver = DeviceFixture::new("device-b");
    bootstrap_game(&cloud, &source, Some(&receiver), GAME_ID, GAME_NAME).await;

    let snapshot = snapshot(SNAPSHOT_ID, None, &source.id, ARCHIVE_BYTES.len());
    source.write_archive(GAME_ID, &snapshot, ARCHIVE_BYTES);
    let snapshots = source.snapshots(GAME_NAME, vec![snapshot.clone()], SNAPSHOT_ID);
    reconcile_game(&cloud, &source, GAME_ID, &snapshots).await;
    (cloud, source, receiver, snapshot)
}

fn assert_no_download_staging_file(device: &DeviceFixture) {
    let staging = device.progress_path.with_extension("staging");
    if staging.exists() {
        assert!(
            std::fs::read_dir(staging)
                .expect("staging directory should be readable")
                .next()
                .is_none(),
            "failed downloads must remove their temporary archive"
        );
    }
}

async fn assert_damaged_download_preserves_local_archive(
    cloud: &FsCloudFixture,
    receiver: &DeviceFixture,
    snapshot: &rgsm_core::backup::Snapshot,
) -> MaterializationError {
    let target = receiver.write_archive(GAME_ID, snapshot, EXISTING_BYTES);
    let before = load_manifest(cloud).await;

    let error = CloudArchiveMaterializer::new(
        cloud.new_operator(),
        receiver.archive_root.clone(),
        receiver.id.clone(),
        receiver.progress_path.clone(),
        MAX_ATTEMPTS,
    )
    .download(GAME_ID, SNAPSHOT_ID)
    .await
    .expect_err("damaged cloud archives must fail materialization");

    assert_eq!(
        std::fs::read(target).expect("preexisting archive should remain readable"),
        EXISTING_BYTES
    );
    assert_no_download_staging_file(receiver);
    let after = load_manifest(cloud).await;
    assert_eq!(after, before);
    assert!(
        !after.games[GAME_ID]
            .local_archives
            .get(&receiver.id)
            .is_some_and(|snapshots| snapshots.contains(SNAPSHOT_ID))
    );
    error
}

#[tokio::test]
async fn missing_cloud_archive_fails_without_replacing_local_archive() {
    let (cloud, _, receiver, snapshot) = published_snapshot().await;
    cloud
        .new_operator()
        .delete(&cloud_archive(GAME_ID, &snapshot))
        .await
        .expect("verified cloud archive should be removable through the Fs operator");

    let error = assert_damaged_download_preserves_local_archive(&cloud, &receiver, &snapshot).await;
    assert!(matches!(
        error,
        MaterializationError::Backend(BackendError::Cloud(error))
            if error.kind() == ErrorKind::NotFound
    ));
}

#[tokio::test]
async fn truncated_cloud_archive_fails_without_replacing_local_archive() {
    let (cloud, _, receiver, snapshot) = published_snapshot().await;
    let remote = cloud_archive(GAME_ID, &snapshot);
    let remote_path = cloud.root().join(&remote);
    let archive = std::fs::OpenOptions::new()
        .write(true)
        .open(&remote_path)
        .expect("Fs cloud archive should be directly truncatable");
    archive
        .set_len(1)
        .expect("Fs cloud archive should truncate through the production root");

    let error = assert_damaged_download_preserves_local_archive(&cloud, &receiver, &snapshot).await;
    assert!(matches!(
        error,
        MaterializationError::Integrity(ArchiveIntegrityError::Mismatch { .. })
    ));
}
#[tokio::test]
async fn corrupt_cloud_archive_fails_without_replacing_local_archive() {
    let (cloud, _, receiver, snapshot) = published_snapshot().await;
    let corruption = vec![b'x'; ARCHIVE_BYTES.len()];
    cloud
        .new_operator()
        .write(&cloud_archive(GAME_ID, &snapshot), corruption)
        .await
        .expect("Fs cloud archive should be overwritable through the production operator");

    let error = assert_damaged_download_preserves_local_archive(&cloud, &receiver, &snapshot).await;
    assert!(matches!(
        error,
        MaterializationError::Integrity(ArchiveIntegrityError::Mismatch { .. })
    ));
}

#[tokio::test]
async fn deleted_game_archive_recreation_cannot_restore_game_state() {
    let (cloud, source, stale_device, snapshot) = published_snapshot().await;
    stale_device.write_archive(GAME_ID, &snapshot, ARCHIVE_BYTES);
    let stale_game = load_manifest(&cloud).await.games[GAME_ID].clone();

    SharedGameDeletion::new(
        cloud.new_operator(),
        source.archive_root.clone(),
        source.id.clone(),
        MAX_ATTEMPTS,
    )
    .delete(GAME_ID, GAME_NAME, true)
    .await
    .expect("permanent deletion should remove the populated Game");

    let remote = cloud_archive(GAME_ID, &snapshot);
    cloud
        .new_operator()
        .write(&remote, ARCHIVE_BYTES)
        .await
        .expect("stale archive object should be recreatable in the Fs root");
    assert!(
        cloud
            .new_operator()
            .exists(&remote)
            .await
            .expect("recreated archive should exist")
    );
    let mut resurrected_manifest = CloudManifest::default();
    resurrected_manifest
        .games
        .insert(GAME_ID.to_string(), stale_game);
    cloud
        .new_operator()
        .write(
            CLOUD_MANIFEST_PATH,
            serde_json::to_vec_pretty(&resurrected_manifest)
                .expect("resurrected Cloud Manifest should serialize"),
        )
        .await
        .expect("stale manifest write after marker validation should be reproducible");

    let stale_snapshots = stale_device.snapshots(GAME_NAME, vec![snapshot.clone()], SNAPSHOT_ID);
    assert!(
        SnapshotSyncCoordinator::new(
            cloud.new_operator(),
            stale_device.archive_root.clone(),
            stale_device.id.clone(),
            stale_device.progress_path.clone(),
            MAX_ATTEMPTS,
        )
        .reconcile_game(
            GAME_ID,
            &stale_snapshots,
            0,
            &Default::default(),
            &CancellationToken::new(),
        )
        .await
        .is_err()
    );

    assert!(
        DeletionRegistryRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
            .load()
            .await
            .expect("deletion registry should reload")
            .deleted_games
            .contains_key(GAME_ID)
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
    assert!(
        !cloud
            .new_operator()
            .exists(&remote)
            .await
            .expect("recreated archive should be removed"),
        "the durable deletion marker must dominate stale archive publication"
    );
}
