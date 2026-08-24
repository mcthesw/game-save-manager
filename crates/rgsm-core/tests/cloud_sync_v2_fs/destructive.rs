use std::collections::BTreeSet;

use rgsm_core::backup::{CreatedBy, archive_path};
use rgsm_core::cloud_sync::v2::{
    CLOUD_MANIFEST_PATH, CloudArchiveMaterializer, CloudLibraryBootstrap, CloudManifestRepository,
    DeletionKind, DeletionRegistryRepository, DeviceProfileRemoval, DeviceProfileRemovalError,
    DeviceProfileRepository, DeviceProfileRepositoryError, SHARED_LIBRARY_PATH,
    SharedLibraryRepository, SnapshotDeletionLifecycle, SnapshotDeletionLifecycleError,
    SnapshotState, SnapshotSyncCoordinator, cloud_archive_path, device_profile_path,
};
use tokio_util::sync::CancellationToken;

use crate::support::{
    DeviceFixture, FsCloudFixture, MAX_ATTEMPTS, cloud_namespace_descriptor, no_baseline, snapshot,
};

const GAME_ID: &str = "example-game";
const GAME_NAME: &str = "Example Game";

async fn bootstrap_game(
    cloud: &FsCloudFixture,
    device_a: &DeviceFixture,
    device_b: Option<&DeviceFixture>,
) {
    let (empty_library, empty_profile) = device_a.empty_library_and_profile();
    CloudLibraryBootstrap::new(cloud.new_operator(), MAX_ATTEMPTS)
        .create_empty(
            &cloud_namespace_descriptor(),
            &empty_library,
            &empty_profile,
        )
        .await
        .expect("empty Fs root should bootstrap");

    let (library, profile_a) = device_a.library_and_profile_with_game(GAME_ID, GAME_NAME);
    SharedLibraryRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
        .compare_replace(&empty_library, &library)
        .await
        .expect("Shared Library should publish");
    DeviceProfileRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
        .publish(&device_a.id, &profile_a)
        .await
        .expect("device A profile should publish");

    if let Some(device_b) = device_b {
        let (_, profile_b) = device_b.library_and_profile_with_game(GAME_ID, GAME_NAME);
        DeviceProfileRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
            .publish(&device_b.id, &profile_b)
            .await
            .expect("device B profile should publish");
    }
}

async fn reconcile(
    cloud: &FsCloudFixture,
    device: &DeviceFixture,
    snapshots: &rgsm_core::backup::GameSnapshots,
) {
    SnapshotSyncCoordinator::new(
        cloud.new_operator(),
        device.archive_root.clone(),
        device.id.clone(),
        device.progress_path.clone(),
        MAX_ATTEMPTS,
    )
    .reconcile_game(
        GAME_ID,
        snapshots,
        0,
        &no_baseline(),
        &CancellationToken::new(),
    )
    .await
    .expect("Snapshot reconciliation should succeed");
}

#[tokio::test]
async fn permanent_deletion_tombstones_shared_state_and_converges_other_device_local_bytes() {
    const SNAPSHOT_ID: &str = "snapshot-a";
    const ARCHIVE_BYTES: &[u8] = b"opaque archive bytes shared by two devices";

    let cloud = FsCloudFixture::new();
    let device_a = DeviceFixture::new("device-a");
    let device_b = DeviceFixture::new("device-b");
    bootstrap_game(&cloud, &device_a, Some(&device_b)).await;

    let shared_snapshot = snapshot(SNAPSHOT_ID, None, &device_a.id, ARCHIVE_BYTES.len());
    let device_a_path = device_a.write_archive(GAME_ID, &shared_snapshot, ARCHIVE_BYTES);
    let snapshots_a = device_a.snapshots(GAME_NAME, vec![shared_snapshot.clone()], SNAPSHOT_ID);
    reconcile(&cloud, &device_a, &snapshots_a).await;

    let snapshots_b = device_b.snapshots(GAME_NAME, vec![shared_snapshot], SNAPSHOT_ID);
    reconcile(&cloud, &device_b, &snapshots_b).await;
    CloudArchiveMaterializer::new(
        cloud.new_operator(),
        device_b.archive_root.clone(),
        device_b.id.clone(),
        device_b.progress_path.clone(),
        MAX_ATTEMPTS,
    )
    .download(GAME_ID, SNAPSHOT_ID)
    .await
    .expect("device B should download the shared archive");
    let device_b_path = archive_path(
        &device_b.archive_root.join(GAME_ID),
        SNAPSHOT_ID,
        rgsm_core::backup::ArchiveFormat::Zip,
    );
    assert_eq!(
        std::fs::read(&device_b_path).expect("device B should materialize the archive"),
        ARCHIVE_BYTES
    );

    let lifecycle_a = SnapshotDeletionLifecycle::new(
        cloud.new_operator(),
        device_a.archive_root.clone(),
        device_a.id.clone(),
        MAX_ATTEMPTS,
    );
    assert!(matches!(
        lifecycle_a
            .delete_snapshot(GAME_ID, SNAPSHOT_ID, false)
            .await,
        Err(SnapshotDeletionLifecycleError::ConfirmationRequired)
    ));
    assert!(device_a_path.exists());
    assert!(device_b_path.exists());

    lifecycle_a
        .delete_snapshot(GAME_ID, SNAPSHOT_ID, true)
        .await
        .expect("confirmed permanent deletion should complete");
    assert!(!device_a_path.exists());
    assert!(
        !cloud
            .new_operator()
            .exists(
                &cloud_archive_path(GAME_ID, SNAPSHOT_ID, rgsm_core::backup::ArchiveFormat::Zip)
                    .expect("cloud archive path should be valid")
            )
            .await
            .expect("cloud archive absence should be observable")
    );
    assert!(
        device_b_path.exists(),
        "another Device removes its local copy only when it next converges"
    );

    let tombstones = SnapshotDeletionLifecycle::new(
        cloud.new_operator(),
        device_b.archive_root.clone(),
        device_b.id.clone(),
        MAX_ATTEMPTS,
    )
    .converge_local_tombstones()
    .await
    .expect("fresh device B lifecycle should converge Tombstones");
    assert_eq!(
        tombstones.get(GAME_ID),
        Some(&BTreeSet::from([SNAPSHOT_ID.to_string()]))
    );
    assert!(!device_b_path.exists());

    lifecycle_a
        .delete_snapshot(GAME_ID, SNAPSHOT_ID, true)
        .await
        .expect("repeated deletion should be idempotent");
    let manifest =
        CloudManifestRepository::new(cloud.new_operator(), CLOUD_MANIFEST_PATH, MAX_ATTEMPTS)
            .load()
            .await
            .expect("final Cloud Manifest should load");
    let game = &manifest.games[GAME_ID];
    assert!(matches!(
        game.snapshots[SNAPSHOT_ID].state,
        SnapshotState::FinalTombstone {
            kind: DeletionKind::User
        }
    ));
    assert!(game.device_heads.is_empty());
    assert!(
        game.local_archives
            .values()
            .all(|items| !items.contains(SNAPSHOT_ID))
    );
}

#[tokio::test]
async fn retention_removes_only_expired_automatic_snapshot_and_keeps_live_branch() {
    const OLD: &str = "snapshot-old";
    const KEPT: &str = "snapshot-kept";
    const HEAD: &str = "snapshot-head";

    let cloud = FsCloudFixture::new();
    let device = DeviceFixture::new("device-a");
    bootstrap_game(&cloud, &device, None).await;

    let mut old = snapshot(OLD, None, &device.id, OLD.len());
    old.created_by = CreatedBy::Timer;
    let mut kept = snapshot(KEPT, Some(OLD), &device.id, KEPT.len());
    kept.created_by = CreatedBy::Timer;
    let mut head = snapshot(HEAD, Some(KEPT), &device.id, HEAD.len());
    head.created_by = CreatedBy::Timer;
    for item in [&old, &kept, &head] {
        device.write_archive(GAME_ID, item, item.date.as_bytes());
    }
    let snapshots = device.snapshots(GAME_NAME, vec![old, kept, head], HEAD);
    reconcile(&cloud, &device, &snapshots).await;

    let coordinator = SnapshotSyncCoordinator::new(
        cloud.new_operator(),
        device.archive_root.clone(),
        device.id.clone(),
        device.progress_path.clone(),
        MAX_ATTEMPTS,
    );
    let outcome = coordinator
        .enforce_retention(GAME_ID, 1)
        .await
        .expect("retention should complete through Fs");
    assert_eq!(outcome.deleted, 1);
    assert_eq!(outcome.tombstones, BTreeSet::from([OLD.to_string()]));

    let fresh_operator = cloud.new_operator();
    let manifest =
        CloudManifestRepository::new(fresh_operator.clone(), CLOUD_MANIFEST_PATH, MAX_ATTEMPTS)
            .load()
            .await
            .expect("retained Cloud Manifest should load");
    let game = &manifest.games[GAME_ID];
    assert!(matches!(
        game.snapshots[OLD].state,
        SnapshotState::FinalTombstone {
            kind: DeletionKind::Retention
        }
    ));
    assert!(game.snapshots[KEPT].state.is_live());
    assert!(game.snapshots[HEAD].state.is_live());
    assert_eq!(
        game.device_heads.get(&device.id).map(String::as_str),
        Some(HEAD)
    );
    for (snapshot_id, exists) in [(OLD, false), (KEPT, true), (HEAD, true)] {
        assert_eq!(
            archive_path(
                &device.archive_root.join(GAME_ID),
                snapshot_id,
                rgsm_core::backup::ArchiveFormat::Zip
            )
            .exists(),
            exists
        );
        assert_eq!(
            fresh_operator
                .exists(
                    &cloud_archive_path(
                        GAME_ID,
                        snapshot_id,
                        rgsm_core::backup::ArchiveFormat::Zip
                    )
                    .expect("cloud archive path should be valid")
                )
                .await
                .expect("cloud archive existence should be readable"),
            exists
        );
    }

    let repeated = coordinator
        .enforce_retention(GAME_ID, 1)
        .await
        .expect("repeated retention should be idempotent");
    assert_eq!(repeated.deleted, 0);
}

#[tokio::test]
async fn profile_removal_preserves_other_device_head_and_blocks_stale_republication() {
    const SNAPSHOT_ID: &str = "snapshot-a";
    const ARCHIVE_BYTES: &[u8] = b"archive retained after profile removal";

    let cloud = FsCloudFixture::new();
    let device_a = DeviceFixture::new("device-a");
    let device_b = DeviceFixture::new("device-b");
    bootstrap_game(&cloud, &device_a, Some(&device_b)).await;

    let shared_snapshot = snapshot(SNAPSHOT_ID, None, &device_a.id, ARCHIVE_BYTES.len());
    device_a.write_archive(GAME_ID, &shared_snapshot, ARCHIVE_BYTES);
    let snapshots_a = device_a.snapshots(GAME_NAME, vec![shared_snapshot.clone()], SNAPSHOT_ID);
    reconcile(&cloud, &device_a, &snapshots_a).await;
    let snapshots_b = device_b.snapshots(GAME_NAME, vec![shared_snapshot], SNAPSHOT_ID);
    reconcile(&cloud, &device_b, &snapshots_b).await;

    let removal =
        DeviceProfileRemoval::new(cloud.new_operator(), device_a.id.clone(), MAX_ATTEMPTS);
    assert!(matches!(
        removal.remove(&device_b.id, false).await,
        Err(DeviceProfileRemovalError::ConfirmationRequired)
    ));
    let (_, stale_profile_b) = device_b.library_and_profile_with_game(GAME_ID, GAME_NAME);

    let outcome = removal
        .remove(&device_b.id, true)
        .await
        .expect("confirmed Device Profile removal should complete");
    assert_eq!(outcome.device_id, device_b.id);
    assert_eq!(outcome.removed_heads, 1);

    let fresh_operator = cloud.new_operator();
    assert!(
        !fresh_operator
            .exists(&device_profile_path(&device_b.id))
            .await
            .expect("removed profile absence should be observable")
    );
    let registry = DeletionRegistryRepository::new(fresh_operator.clone(), MAX_ATTEMPTS)
        .load()
        .await
        .expect("Deletion Registry should load");
    assert!(registry.deleted_profiles.contains_key(&device_b.id));
    let manifest =
        CloudManifestRepository::new(fresh_operator.clone(), CLOUD_MANIFEST_PATH, MAX_ATTEMPTS)
            .load()
            .await
            .expect("Cloud Manifest should load");
    let game = &manifest.games[GAME_ID];
    assert_eq!(
        game.device_heads.get(&device_a.id).map(String::as_str),
        Some(SNAPSHOT_ID)
    );
    assert!(!game.device_heads.contains_key(&device_b.id));
    assert!(game.snapshots[SNAPSHOT_ID].state.is_live());
    assert!(
        fresh_operator
            .exists(
                &cloud_archive_path(GAME_ID, SNAPSHOT_ID, rgsm_core::backup::ArchiveFormat::Zip)
                    .expect("cloud archive path should be valid")
            )
            .await
            .expect("shared archive should remain readable")
    );
    assert!(matches!(
        DeviceProfileRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
            .publish(&device_b.id, &stale_profile_b)
            .await,
        Err(DeviceProfileRepositoryError::Deleted(device)) if device == device_b.id
    ));

    let repeated = removal
        .remove(&device_b.id, true)
        .await
        .expect("repeated Profile removal should be idempotent");
    assert_eq!(repeated.removed_heads, 0);
    assert_eq!(
        SharedLibraryRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
            .load()
            .await
            .expect("Shared Library should remain")
            .games
            .len(),
        1
    );
    assert!(
        cloud
            .new_operator()
            .exists(SHARED_LIBRARY_PATH)
            .await
            .expect("Shared Library object should remain")
    );
}
