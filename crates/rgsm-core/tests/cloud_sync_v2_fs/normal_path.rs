use rgsm_core::backup::{ArchiveFormat, archive_path};
use rgsm_core::cloud_sync::v2::{
    CLOUD_MANIFEST_PATH, CloudArchiveMaterializer, CloudLibraryBootstrap, CloudManifestRepository,
    CloudNamespaceClassification, DELETION_REGISTRY_PATH, DeviceProfileRepository,
    LocalArchiveEviction, SHARED_LIBRARY_PATH, SharedLibraryRepository, SnapshotState,
    SnapshotSyncCoordinator, V2_NAMESPACE_DESCRIPTOR_PATH, cloud_archive_path, device_profile_path,
};
use tokio_util::sync::CancellationToken;

use crate::support::{DeviceFixture, FsCloudFixture, MAX_ATTEMPTS, no_baseline, snapshot};

#[tokio::test]
async fn bootstrap_persists_complete_v2_namespace_across_fresh_operators() {
    let cloud = FsCloudFixture::new();
    let device = DeviceFixture::new("device-a");
    let (shared_library, device_profile) = device.empty_library_and_profile();

    CloudLibraryBootstrap::new(cloud.new_operator(), MAX_ATTEMPTS)
        .create_empty(&shared_library, &device_profile)
        .await
        .expect("empty Fs root should bootstrap");

    let fresh_operator = cloud.new_operator();
    let classification = CloudLibraryBootstrap::new(fresh_operator.clone(), MAX_ATTEMPTS)
        .inspect()
        .await
        .expect("persisted namespace should classify");

    let CloudNamespaceClassification::SupportedV2 {
        descriptor,
        shared_library: stored_library,
        manifest,
    } = classification
    else {
        panic!("bootstrapped Fs root should classify as V2");
    };
    assert_eq!(descriptor, Default::default());
    assert_eq!(stored_library, shared_library);
    assert_eq!(manifest, Default::default());

    for path in [
        V2_NAMESPACE_DESCRIPTOR_PATH,
        SHARED_LIBRARY_PATH,
        CLOUD_MANIFEST_PATH,
        DELETION_REGISTRY_PATH,
        &device_profile_path(&device.id),
    ] {
        assert!(
            fresh_operator
                .exists(path)
                .await
                .expect("required object existence should be readable"),
            "required V2 object should persist: {path}"
        );
    }

    assert!(!cloud.root().join("game-save-manager").exists());
}

#[tokio::test]
async fn single_device_snapshot_round_trip_survives_fresh_operators() {
    const GAME_ID: &str = "example-game";
    const GAME_NAME: &str = "Example Game";
    const SNAPSHOT_ID: &str = "snapshot-a";
    const ARCHIVE_BYTES: &[u8] = b"opaque archive bytes from device A";

    let cloud = FsCloudFixture::new();
    let device = DeviceFixture::new("device-a");
    let (empty_library, empty_profile) = device.empty_library_and_profile();
    CloudLibraryBootstrap::new(cloud.new_operator(), MAX_ATTEMPTS)
        .create_empty(&empty_library, &empty_profile)
        .await
        .expect("empty Fs root should bootstrap");

    let (library, profile) = device.library_and_profile_with_game(GAME_ID, GAME_NAME);
    SharedLibraryRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
        .compare_replace(&empty_library, &library)
        .await
        .expect("Shared Library should publish");
    DeviceProfileRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
        .publish(&device.id, &profile)
        .await
        .expect("Device Profile should publish");

    let local_snapshot = snapshot(SNAPSHOT_ID, None, &device.id, ARCHIVE_BYTES.len());
    let local_path = device.write_archive(GAME_ID, &local_snapshot, ARCHIVE_BYTES);
    let local_snapshots = device.snapshots(GAME_NAME, vec![local_snapshot.clone()], SNAPSHOT_ID);
    let first = SnapshotSyncCoordinator::new(
        cloud.new_operator(),
        device.archive_root.clone(),
        device.id.clone(),
        device.progress_path.clone(),
        MAX_ATTEMPTS,
    )
    .reconcile_game(
        GAME_ID,
        &local_snapshots,
        0,
        &no_baseline(),
        &CancellationToken::new(),
    )
    .await
    .expect("first reconciliation should publish and upload");
    assert_eq!(first.published, 1);
    assert_eq!(first.uploaded, 1);
    assert_eq!(first.downloaded, 0);

    let fresh_operator = cloud.new_operator();
    let manifest =
        CloudManifestRepository::new(fresh_operator.clone(), CLOUD_MANIFEST_PATH, MAX_ATTEMPTS)
            .load()
            .await
            .expect("Cloud Manifest should load through a fresh Operator");
    let game = manifest
        .games
        .get(GAME_ID)
        .expect("synced Game should exist");
    assert_eq!(
        game.device_heads.get(&device.id).map(String::as_str),
        Some(SNAPSHOT_ID)
    );
    assert!(
        game.local_archives
            .get(&device.id)
            .is_some_and(|items| items.contains(SNAPSHOT_ID))
    );
    let node = game
        .snapshots
        .get(SNAPSHOT_ID)
        .expect("synced Snapshot should exist");
    let SnapshotState::Live(live) = &node.state else {
        panic!("synced Snapshot should be live");
    };
    assert!(live.cloud_archive_verified);
    assert_eq!(
        live.integrity.as_ref().map(|integrity| integrity.size),
        Some(ARCHIVE_BYTES.len() as u64)
    );
    let remote_path = cloud_archive_path(GAME_ID, SNAPSHOT_ID, ArchiveFormat::Zip)
        .expect("cloud archive path should be valid");
    assert_eq!(
        fresh_operator
            .read(&remote_path)
            .await
            .expect("cloud archive should be readable")
            .to_vec(),
        ARCHIVE_BYTES
    );

    assert!(
        LocalArchiveEviction::new(
            cloud.new_operator(),
            device.archive_root.clone(),
            device.id.clone(),
            MAX_ATTEMPTS,
        )
        .evict(GAME_ID, SNAPSHOT_ID)
        .await
        .expect("local archive should evict")
    );
    assert!(!local_path.exists());

    CloudArchiveMaterializer::new(
        cloud.new_operator(),
        device.archive_root.clone(),
        device.id.clone(),
        device.progress_path.clone(),
        MAX_ATTEMPTS,
    )
    .download(GAME_ID, SNAPSHOT_ID)
    .await
    .expect("cloud archive should materialize");
    assert_eq!(
        std::fs::read(&local_path).expect("materialized archive should be readable"),
        ARCHIVE_BYTES
    );
    assert_eq!(
        local_path,
        archive_path(
            &device.archive_root.join(GAME_ID),
            SNAPSHOT_ID,
            ArchiveFormat::Zip
        )
    );

    let repeated = SnapshotSyncCoordinator::new(
        cloud.new_operator(),
        device.archive_root.clone(),
        device.id.clone(),
        device.progress_path.clone(),
        MAX_ATTEMPTS,
    )
    .reconcile_game(
        GAME_ID,
        &local_snapshots,
        0,
        &no_baseline(),
        &CancellationToken::new(),
    )
    .await
    .expect("repeated reconciliation should succeed");
    assert_eq!(repeated.published, 0);
    assert_eq!(repeated.uploaded, 0);
    assert_eq!(repeated.downloaded, 0);
}
