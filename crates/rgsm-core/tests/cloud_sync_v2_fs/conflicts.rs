use opendal::ErrorKind;
use rgsm_core::cloud_sync::v2::{
    AcceptRemoteProgressError, CLOUD_MANIFEST_PATH, CloudManifestRepository,
    KeepLocalProgressError, ManifestError, MaterializationError, ProgressRelation, SnapshotState,
    V2ConflictInspector, V2ConflictResolver, V2RemoteProgressResolver,
};
use rgsm_core::preclude::BackendError;

use crate::support::{
    DeviceFixture, FsCloudFixture, MAX_ATTEMPTS, bootstrap_game, cloud_archive, load_manifest,
    reconcile_game, snapshot,
};

const GAME_ID: &str = "example-game";
const GAME_NAME: &str = "Example Game";
const PARENT_ID: &str = "parent";
const LOCAL_HEAD_ID: &str = "device-a-head";
const REMOTE_HEAD_ID: &str = "device-b-head";
const PARENT_BYTES: &[u8] = b"shared parent archive";
const LOCAL_BYTES: &[u8] = b"device A descendant archive";
const REMOTE_BYTES: &[u8] = b"device B descendant archive";

struct DivergedHistories {
    cloud: FsCloudFixture,
    device_a: DeviceFixture,
    device_b: DeviceFixture,
    parent: rgsm_core::backup::Snapshot,
    local_head: rgsm_core::backup::Snapshot,
    remote_head: rgsm_core::backup::Snapshot,
    local_a: rgsm_core::backup::GameSnapshots,
}

async fn divergent_histories() -> DivergedHistories {
    let cloud = FsCloudFixture::new();
    let device_a = DeviceFixture::new("device-a");
    let device_b = DeviceFixture::new("device-b");
    bootstrap_game(&cloud, &device_a, Some(&device_b), GAME_ID, GAME_NAME).await;

    let parent = snapshot(PARENT_ID, None, &device_a.id, PARENT_BYTES.len());
    device_a.write_archive(GAME_ID, &parent, PARENT_BYTES);
    let parent_only = device_a.snapshots(GAME_NAME, vec![parent.clone()], PARENT_ID);
    reconcile_game(&cloud, &device_a, GAME_ID, &parent_only).await;

    let local_head = snapshot(
        LOCAL_HEAD_ID,
        Some(PARENT_ID),
        &device_a.id,
        LOCAL_BYTES.len(),
    );
    device_a.write_archive(GAME_ID, &local_head, LOCAL_BYTES);
    let local_a = device_a.snapshots(
        GAME_NAME,
        vec![parent.clone(), local_head.clone()],
        LOCAL_HEAD_ID,
    );
    reconcile_game(&cloud, &device_a, GAME_ID, &local_a).await;

    let remote_head = snapshot(
        REMOTE_HEAD_ID,
        Some(PARENT_ID),
        &device_b.id,
        REMOTE_BYTES.len(),
    );
    device_b.write_archive(GAME_ID, &remote_head, REMOTE_BYTES);
    let local_b = device_b.snapshots(
        GAME_NAME,
        vec![parent.clone(), remote_head.clone()],
        REMOTE_HEAD_ID,
    );
    reconcile_game(&cloud, &device_b, GAME_ID, &local_b).await;

    DivergedHistories {
        cloud,
        device_a,
        device_b,
        parent,
        local_head,
        remote_head,
        local_a,
    }
}

async fn local_only_divergence() -> DivergedHistories {
    let cloud = FsCloudFixture::new();
    let device_a = DeviceFixture::new("device-a");
    let device_b = DeviceFixture::new("device-b");
    bootstrap_game(&cloud, &device_a, Some(&device_b), GAME_ID, GAME_NAME).await;

    let parent = snapshot(PARENT_ID, None, &device_a.id, PARENT_BYTES.len());
    device_a.write_archive(GAME_ID, &parent, PARENT_BYTES);
    let parent_only = device_a.snapshots(GAME_NAME, vec![parent.clone()], PARENT_ID);
    reconcile_game(&cloud, &device_a, GAME_ID, &parent_only).await;

    let remote_head = snapshot(
        REMOTE_HEAD_ID,
        Some(PARENT_ID),
        &device_b.id,
        REMOTE_BYTES.len(),
    );
    device_b.write_archive(GAME_ID, &remote_head, REMOTE_BYTES);
    let local_b = device_b.snapshots(
        GAME_NAME,
        vec![parent.clone(), remote_head.clone()],
        REMOTE_HEAD_ID,
    );
    reconcile_game(&cloud, &device_b, GAME_ID, &local_b).await;

    let local_head = snapshot(
        LOCAL_HEAD_ID,
        Some(PARENT_ID),
        &device_a.id,
        LOCAL_BYTES.len(),
    );
    device_a.write_archive(GAME_ID, &local_head, LOCAL_BYTES);
    let local_a = device_a.snapshots(
        GAME_NAME,
        vec![parent.clone(), local_head.clone()],
        LOCAL_HEAD_ID,
    );

    DivergedHistories {
        cloud,
        device_a,
        device_b,
        parent,
        local_head,
        remote_head,
        local_a,
    }
}

fn inspector(histories: &DivergedHistories) -> V2ConflictInspector {
    V2ConflictInspector::new(
        histories.cloud.new_operator(),
        histories.device_a.archive_root.clone(),
        histories.device_a.id.clone(),
        MAX_ATTEMPTS,
    )
}

#[tokio::test]
async fn conflict_review_reports_divergent_device_positions() {
    let histories = divergent_histories().await;

    let review = inspector(&histories)
        .review(GAME_ID, &histories.local_a)
        .await
        .expect("divergent positions should be reviewable");

    assert!(review.requires_choice);
    assert_eq!(
        review
            .local
            .as_ref()
            .map(|local| local.snapshot_id.as_str()),
        Some(LOCAL_HEAD_ID)
    );
    // The current Device's head is excluded from remote candidates because
    // the local Current Position is authoritative. Only device B's head appears.
    assert_eq!(review.candidates.len(), 1);

    let remote = review
        .candidates
        .iter()
        .find(|candidate| candidate.snapshot_id == REMOTE_HEAD_ID)
        .expect("device B head should be advertised");
    assert_eq!(remote.devices, vec![histories.device_b.id.clone()]);
    assert_eq!(remote.relation, ProgressRelation::DifferentProgress);
    assert_eq!(remote.common_ancestor.as_deref(), Some(PARENT_ID));
    assert!(!remote.local_available);
    assert!(remote.cloud_available);
}

#[tokio::test]
async fn keep_local_publishes_complete_lineage_and_preserves_remote_position() {
    let histories = local_only_divergence().await;
    let review = inspector(&histories)
        .review(GAME_ID, &histories.local_a)
        .await
        .expect("divergent positions should be reviewable");

    let before = load_manifest(&histories.cloud).await;
    assert!(
        !before.games[GAME_ID].snapshots.contains_key(LOCAL_HEAD_ID),
        "the selected local Snapshot must be unpublished before resolution"
    );
    assert!(
        !histories
            .cloud
            .new_operator()
            .exists(&cloud_archive(GAME_ID, &histories.local_head))
            .await
            .expect("cloud archive absence should be observable"),
        "the selected local Archive must be unpublished before resolution"
    );

    let outcome = V2ConflictResolver::new(
        histories.cloud.new_operator(),
        histories.device_a.archive_root.clone(),
        histories.device_a.id.clone(),
        histories.device_a.progress_path.clone(),
        MAX_ATTEMPTS,
    )
    .keep_local(
        GAME_ID,
        review.manifest_revision,
        LOCAL_HEAD_ID,
        &histories.local_a,
    )
    .await
    .expect("keeping local progress should publish its lineage");

    let manifest = load_manifest(&histories.cloud).await;
    let game = manifest
        .games
        .get(GAME_ID)
        .expect("game should remain in manifest");
    assert_eq!(outcome.prepared_snapshots, 2);
    assert_eq!(outcome.uploaded_archives, 1);
    assert!(outcome.manifest_revision > review.manifest_revision);
    assert_eq!(
        game.device_heads
            .get(&histories.device_a.id)
            .map(String::as_str),
        Some(LOCAL_HEAD_ID)
    );
    assert_eq!(
        game.device_heads
            .get(&histories.device_b.id)
            .map(String::as_str),
        Some(REMOTE_HEAD_ID)
    );
    for (snapshot, expected) in [
        (&histories.parent, PARENT_BYTES),
        (&histories.local_head, LOCAL_BYTES),
    ] {
        let node = game
            .snapshots
            .get(&snapshot.date)
            .expect("lineage node should persist");
        assert!(matches!(
            &node.state,
            SnapshotState::Live(live) if live.cloud_archive_verified
        ));
        assert_eq!(
            histories
                .cloud
                .new_operator()
                .read(&cloud_archive(GAME_ID, snapshot))
                .await
                .expect("published archive should be readable")
                .to_vec(),
            expected
        );
    }
    assert_eq!(
        histories
            .cloud
            .new_operator()
            .read(&cloud_archive(GAME_ID, &histories.remote_head))
            .await
            .expect("remote device archive should remain readable")
            .to_vec(),
        REMOTE_BYTES
    );
}

#[tokio::test]
async fn accept_remote_materializes_selected_archive_and_moves_only_current_device_position() {
    let histories = divergent_histories().await;
    let review = inspector(&histories)
        .review(GAME_ID, &histories.local_a)
        .await
        .expect("divergent positions should be reviewable");

    let resolver = V2RemoteProgressResolver::new(
        histories.cloud.new_operator(),
        histories.device_a.archive_root.clone(),
        histories.device_a.id.clone(),
        histories.device_a.progress_path.clone(),
        MAX_ATTEMPTS,
    );
    let prepared = resolver
        .prepare(
            GAME_ID,
            review.manifest_revision,
            Some(LOCAL_HEAD_ID),
            REMOTE_HEAD_ID,
            &histories.local_a,
        )
        .await
        .expect("advertised remote progress should materialize");
    assert_eq!(prepared.selected_snapshot_id, REMOTE_HEAD_ID);
    assert_eq!(
        prepared
            .lineage
            .last()
            .map(|snapshot| snapshot.date.as_str()),
        Some(REMOTE_HEAD_ID)
    );
    let revision = resolver
        .commit_current_device_head(GAME_ID, REMOTE_HEAD_ID)
        .await
        .expect("materialized remote progress should become device A head");

    assert_eq!(
        std::fs::read(
            histories
                .device_a
                .archive_path(GAME_ID, &histories.remote_head)
        )
        .expect("selected archive should materialize locally"),
        REMOTE_BYTES
    );
    let manifest = load_manifest(&histories.cloud).await;
    let game = manifest
        .games
        .get(GAME_ID)
        .expect("game should remain in manifest");
    assert_eq!(manifest.revision, revision);
    assert!(manifest.revision > review.manifest_revision);
    assert_eq!(
        game.device_heads
            .get(&histories.device_a.id)
            .map(String::as_str),
        Some(REMOTE_HEAD_ID)
    );
    assert_eq!(
        game.device_heads
            .get(&histories.device_b.id)
            .map(String::as_str),
        Some(REMOTE_HEAD_ID)
    );
}

#[tokio::test]
async fn keep_local_rejects_stale_review_without_persisted_change() {
    let histories = divergent_histories().await;
    let review = inspector(&histories)
        .review(GAME_ID, &histories.local_a)
        .await
        .expect("divergent positions should be reviewable");
    CloudManifestRepository::new(
        histories.cloud.new_operator(),
        CLOUD_MANIFEST_PATH,
        MAX_ATTEMPTS,
    )
    .mutate(|_| Ok::<_, ManifestError>(()))
    .await
    .expect("independent manifest change should advance the revision");
    let before = load_manifest(&histories.cloud).await;

    let error = V2ConflictResolver::new(
        histories.cloud.new_operator(),
        histories.device_a.archive_root.clone(),
        histories.device_a.id.clone(),
        histories.device_a.progress_path.clone(),
        MAX_ATTEMPTS,
    )
    .keep_local(
        GAME_ID,
        review.manifest_revision,
        LOCAL_HEAD_ID,
        &histories.local_a,
    )
    .await
    .expect_err("stale review must not persist a replacement");

    assert!(matches!(error, KeepLocalProgressError::StaleReview { .. }));
    assert_eq!(load_manifest(&histories.cloud).await, before);
    assert_eq!(
        std::fs::read(
            histories
                .device_a
                .archive_path(GAME_ID, &histories.local_head)
        )
        .expect("existing local archive should remain unchanged"),
        LOCAL_BYTES
    );
}

#[tokio::test]
async fn accept_remote_rejects_unavailable_candidate_without_persisted_change() {
    let histories = divergent_histories().await;
    let review = inspector(&histories)
        .review(GAME_ID, &histories.local_a)
        .await
        .expect("divergent positions should be reviewable");
    histories
        .cloud
        .new_operator()
        .delete(&cloud_archive(GAME_ID, &histories.remote_head))
        .await
        .expect("remote archive should be removable through the Fs operator");
    let before = load_manifest(&histories.cloud).await;

    let result = V2RemoteProgressResolver::new(
        histories.cloud.new_operator(),
        histories.device_a.archive_root.clone(),
        histories.device_a.id.clone(),
        histories.device_a.progress_path.clone(),
        MAX_ATTEMPTS,
    )
    .prepare(
        GAME_ID,
        review.manifest_revision,
        Some(LOCAL_HEAD_ID),
        REMOTE_HEAD_ID,
        &histories.local_a,
    )
    .await;
    let Err(error) = result else {
        panic!("missing remote archive must not replace a local archive");
    };

    assert!(matches!(
        error,
        AcceptRemoteProgressError::Materialization(MaterializationError::Backend(
            BackendError::Cloud(error)
        )) if error.kind() == ErrorKind::NotFound
    ));
    assert_eq!(load_manifest(&histories.cloud).await, before);
    assert!(
        !histories
            .device_a
            .archive_path(GAME_ID, &histories.remote_head)
            .exists()
    );
    let fresh_review = inspector(&histories)
        .review(GAME_ID, &histories.local_a)
        .await
        .expect("manifest remains reviewable after download failure");
    assert!(
        !fresh_review
            .candidates
            .iter()
            .find(|candidate| candidate.snapshot_id == REMOTE_HEAD_ID)
            .expect("advertised candidate should remain visible")
            .local_available
    );
    assert_eq!(
        std::fs::read(
            histories
                .device_a
                .archive_path(GAME_ID, &histories.local_head)
        )
        .expect("existing local archive should remain unchanged"),
        LOCAL_BYTES
    );
}
