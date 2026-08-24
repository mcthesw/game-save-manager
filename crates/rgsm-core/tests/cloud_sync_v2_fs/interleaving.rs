use rgsm_core::cloud_sync::v2::{
    CLOUD_MANIFEST_PATH, CloudLibraryBootstrap, CloudManifestRepository, DeviceProfileRepository,
    SharedLibraryRepository, SharedLibraryRepositoryError, SnapshotSyncCoordinator,
};
use tokio_util::sync::CancellationToken;

use crate::support::{
    DeviceFixture, FsCloudFixture, MAX_ATTEMPTS, cloud_namespace_descriptor, no_baseline, snapshot,
};

#[tokio::test]
async fn two_devices_rebase_stale_library_changes_and_preserve_independent_heads() {
    const GAME_ID: &str = "example-game";
    const GAME_NAME: &str = "Example Game";
    const OTHER_GAME_ID: &str = "other-game";
    const SNAPSHOT_A: &str = "snapshot-a";
    const SNAPSHOT_B: &str = "snapshot-b";
    const ARCHIVE_A: &[u8] = b"opaque archive bytes from device A";
    const ARCHIVE_B: &[u8] = b"opaque archive bytes from device B";

    let cloud = FsCloudFixture::new();
    let device_a = DeviceFixture::new("device-a");
    let device_b = DeviceFixture::new("device-b");
    let (empty_library, empty_profile) = device_a.empty_library_and_profile();
    CloudLibraryBootstrap::new(cloud.new_operator(), MAX_ATTEMPTS)
        .create_empty(
            &cloud_namespace_descriptor(),
            &empty_library,
            &empty_profile,
        )
        .await
        .expect("empty Fs root should bootstrap");

    let repository_a = SharedLibraryRepository::new(cloud.new_operator(), MAX_ATTEMPTS);
    let repository_b = SharedLibraryRepository::new(cloud.new_operator(), MAX_ATTEMPTS);
    let expected_a = repository_a
        .load()
        .await
        .expect("device A should load Shared Library");
    let expected_b = repository_b
        .load()
        .await
        .expect("device B should load the same Shared Library");
    let (library_a, profile_a) = device_a.library_and_profile_with_game(GAME_ID, GAME_NAME);
    let (library_b_only, _) = device_b.library_and_profile_with_game(OTHER_GAME_ID, "Other Game");
    repository_a
        .compare_replace(&expected_a, &library_a)
        .await
        .expect("device A should publish its Shared Library change");
    assert!(matches!(
        repository_b
            .compare_replace(&expected_b, &library_b_only)
            .await,
        Err(SharedLibraryRepositoryError::Stale)
    ));

    let current = repository_b
        .load()
        .await
        .expect("device B should reload after stale replacement");
    let mut merged = current.clone();
    merged.games.extend(library_b_only.games);
    repository_b
        .compare_replace(&current, &merged)
        .await
        .expect("device B should publish its rebased change");
    let final_library = SharedLibraryRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
        .load()
        .await
        .expect("rebased Shared Library should persist");
    assert_eq!(final_library, merged);

    let (_, profile_b) = device_b.library_and_profile_with_game(GAME_ID, GAME_NAME);
    DeviceProfileRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
        .publish(&device_a.id, &profile_a)
        .await
        .expect("device A profile should publish");
    DeviceProfileRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
        .publish(&device_b.id, &profile_b)
        .await
        .expect("device B profile should publish");

    let snapshot_a = snapshot(SNAPSHOT_A, None, &device_a.id, ARCHIVE_A.len());
    device_a.write_archive(GAME_ID, &snapshot_a, ARCHIVE_A);
    let snapshots_a = device_a.snapshots(GAME_NAME, vec![snapshot_a.clone()], SNAPSHOT_A);
    SnapshotSyncCoordinator::new(
        cloud.new_operator(),
        device_a.archive_root.clone(),
        device_a.id.clone(),
        device_a.progress_path.clone(),
        MAX_ATTEMPTS,
    )
    .reconcile_game(
        GAME_ID,
        &snapshots_a,
        0,
        &no_baseline(),
        &CancellationToken::new(),
    )
    .await
    .expect("device A should publish its Snapshot");

    let snapshot_b = snapshot(SNAPSHOT_B, Some(SNAPSHOT_A), &device_b.id, ARCHIVE_B.len());
    device_b.write_archive(GAME_ID, &snapshot_b, ARCHIVE_B);
    let snapshots_b = device_b.snapshots(GAME_NAME, vec![snapshot_a, snapshot_b], SNAPSHOT_B);
    SnapshotSyncCoordinator::new(
        cloud.new_operator(),
        device_b.archive_root.clone(),
        device_b.id.clone(),
        device_b.progress_path.clone(),
        MAX_ATTEMPTS,
    )
    .reconcile_game(
        GAME_ID,
        &snapshots_b,
        0,
        &no_baseline(),
        &CancellationToken::new(),
    )
    .await
    .expect("device B should publish its child Snapshot");

    SnapshotSyncCoordinator::new(
        cloud.new_operator(),
        device_a.archive_root.clone(),
        device_a.id.clone(),
        device_a.progress_path.clone(),
        MAX_ATTEMPTS,
    )
    .reconcile_game(
        GAME_ID,
        &snapshots_a,
        0,
        &no_baseline(),
        &CancellationToken::new(),
    )
    .await
    .expect("device A should reconcile after device B");

    let manifest =
        CloudManifestRepository::new(cloud.new_operator(), CLOUD_MANIFEST_PATH, MAX_ATTEMPTS)
            .load()
            .await
            .expect("final Cloud Manifest should load");
    let game = manifest
        .games
        .get(GAME_ID)
        .expect("interleaved Game should exist");
    assert_eq!(
        game.device_heads.get(&device_a.id).map(String::as_str),
        Some(SNAPSHOT_A)
    );
    assert_eq!(
        game.device_heads.get(&device_b.id).map(String::as_str),
        Some(SNAPSHOT_B)
    );
    assert!(game.snapshots.contains_key(SNAPSHOT_A));
    assert!(game.snapshots.contains_key(SNAPSHOT_B));
}
