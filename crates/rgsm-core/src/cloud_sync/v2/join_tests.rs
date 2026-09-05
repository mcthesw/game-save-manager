use std::collections::BTreeMap;
use std::sync::Mutex as StdMutex;

use async_trait::async_trait;

use super::*;
use crate::cloud_sync::v2::{
    CLOUD_MANIFEST_PATH, CloudManifest, CloudNamespaceDescriptor, NamespaceTransport,
    V2_NAMESPACE_DESCRIPTOR_PATH,
};
use crate::config::{
    ConfigurationOwners, SharedSaveUnit, SharedSaveUnitSource, SharedSnapshotRetentionPolicy,
    V2_CONFIG_SCHEMA_VERSION,
};

const LIBRARY_ID: &str = "11111111-1111-4111-8111-111111111111";

#[tokio::test]
async fn conflicting_definitions_require_a_choice_before_any_write() {
    let local = library(vec![game("same-id", "Local", 2)]);
    let cloud = library(vec![game("same-id", "Cloud", 1)]);
    let join = CloudLibraryJoin::with_transport(transport(&cloud), 2);
    assert!(matches!(
        join.join(&local, &profile(&local), &[], false).await,
        Err(CloudLibraryJoinError::DecisionRequired(_))
    ));
    assert!(join.transport.writes.lock().unwrap().is_empty());
}

#[tokio::test]
async fn keep_cloud_rejects_a_definition_changed_after_review() {
    let local = library(vec![game("same-id", "Local", 2)]);
    let reviewed = library(vec![game("same-id", "Cloud", 1)]);
    let review = build_review(&local, &reviewed).unwrap();
    let changed = library(vec![game("same-id", "Changed later", 3)]);
    let join = CloudLibraryJoin::with_transport(transport(&changed), 2);
    let decision = JoinGameDecision {
        local_game_id: "same-id".into(),
        local_fingerprint: review.items[0].local_fingerprint.clone(),
        cloud_fingerprint: review.items[0].cloud_fingerprint.clone(),
        action: JoinGameAction::KeepCloud,
    };
    assert!(matches!(
        join.join(&local, &profile(&local), &[decision], false)
            .await,
        Err(CloudLibraryJoinError::TargetChanged(_))
    ));
    assert!(join.transport.writes.lock().unwrap().is_empty());
}

#[tokio::test]
async fn equal_definitions_and_same_titles_with_different_ids_do_not_require_a_choice() {
    let local = library(vec![
        game("same-id", "Same", 1),
        game("local-id", "Title", 2),
    ]);
    let cloud = library(vec![
        game("cloud-id", "Title", 3),
        game("same-id", "Same", 1),
    ]);
    let join = CloudLibraryJoin::with_transport(transport(&cloud), 2);
    let result = join
        .join(&local, &profile(&local), &[], false)
        .await
        .unwrap();
    assert_eq!(result.shared_library, cloud);
    assert_eq!(
        join.transport.writes.lock().unwrap().as_slice(),
        [device_profile_path("device")]
    );
}

#[tokio::test]
async fn unrelated_cloud_edits_do_not_invalidate_a_reviewed_choice() {
    let local = library(vec![game("same-id", "Local", 2)]);
    let reviewed = library(vec![game("same-id", "Cloud", 1)]);
    let review = build_review(&local, &reviewed).unwrap();
    let mut changed = reviewed;
    changed.games.push(game("other-id", "Added later", 3));
    changed
        .games
        .sort_by(|left, right| left.storage_key.cmp(&right.storage_key));
    let join = CloudLibraryJoin::with_transport(transport(&changed), 2);
    let decision = JoinGameDecision {
        local_game_id: "same-id".into(),
        local_fingerprint: review.items[0].local_fingerprint.clone(),
        cloud_fingerprint: review.items[0].cloud_fingerprint.clone(),
        action: JoinGameAction::KeepCloud,
    };
    let result = join
        .join(&local, &profile(&local), &[decision], false)
        .await
        .unwrap();
    assert_eq!(result.shared_library, changed);
}

#[tokio::test]
async fn keep_cloud_rejects_a_definition_removed_after_review() {
    let local = library(vec![game("same-id", "Local", 2)]);
    let reviewed = library(vec![game("same-id", "Cloud", 1)]);
    let item = build_review(&local, &reviewed).unwrap().items.remove(0);
    let join = CloudLibraryJoin::with_transport(transport(&library(vec![])), 2);
    let decision = JoinGameDecision {
        local_game_id: item.local_game_id,
        local_fingerprint: item.local_fingerprint,
        cloud_fingerprint: item.cloud_fingerprint,
        action: JoinGameAction::KeepCloud,
    };
    assert!(matches!(
        join.join(&local, &profile(&local), &[decision], false)
            .await,
        Err(CloudLibraryJoinError::TargetChanged(_))
    ));
    assert!(join.transport.writes.lock().unwrap().is_empty());
}

#[tokio::test]
async fn same_title_does_not_supply_an_unrelated_definition_fingerprint() {
    let local = library(vec![game("local-id", "Title", 2)]);
    let cloud = library(vec![game("remote-id", "Title", 1)]);
    let item = build_review(&local, &cloud).unwrap().items.remove(0);
    assert_eq!(
        item.classification,
        GameJoinClassification::PossibleDuplicate
    );
    assert!(item.cloud_fingerprint.is_none());
    let join = CloudLibraryJoin::with_transport(transport(&cloud), 2);
    let decision = JoinGameDecision {
        local_game_id: item.local_game_id,
        local_fingerprint: item.local_fingerprint,
        cloud_fingerprint: item.cloud_fingerprint,
        action: JoinGameAction::KeepCloud,
    };
    let result = join
        .join(&local, &profile(&local), &[decision], false)
        .await
        .unwrap();
    assert_eq!(result.shared_library, cloud);
}

#[derive(Default)]
struct FakeTransport {
    objects: StdMutex<BTreeMap<String, Vec<u8>>>,
    writes: StdMutex<Vec<String>>,
}

#[async_trait]
impl NamespaceTransport for FakeTransport {
    async fn read(&self, path: &str) -> Result<Option<Vec<u8>>, opendal::Error> {
        Ok(self.objects.lock().unwrap().get(path).cloned())
    }

    async fn list_sample(&self, prefix: &str, limit: usize) -> Result<Vec<String>, opendal::Error> {
        Ok(self
            .objects
            .lock()
            .unwrap()
            .keys()
            .filter(|path| prefix == "/" || path.starts_with(prefix))
            .take(limit)
            .cloned()
            .collect())
    }
}

#[async_trait]
impl CloudLibraryTransport for FakeTransport {
    async fn write(&self, path: &str, bytes: &[u8]) -> Result<(), opendal::Error> {
        self.writes.lock().unwrap().push(path.to_string());
        self.objects
            .lock()
            .unwrap()
            .insert(path.to_string(), bytes.to_vec());
        Ok(())
    }
}

fn game(id: &str, name: &str, unit_id: u32) -> SharedGame {
    SharedGame {
        name: name.into(),
        storage_key: id.into(),
        save_units: vec![SharedSaveUnit {
            id: unit_id,
            source: SharedSaveUnitSource::Concrete {
                unit_type: crate::backup::SaveUnitType::Folder,
            },
        }],
        next_save_unit_id: unit_id + 1,
        ludusavi_meta: None,
        snapshot_retention: None,
    }
}

fn library(games: Vec<SharedGame>) -> SharedLibrary {
    SharedLibrary {
        schema_version: V2_CONFIG_SCHEMA_VERSION,
        games,
    }
}

fn transport(cloud: &SharedLibrary) -> FakeTransport {
    let transport = FakeTransport::default();
    let mut objects = transport.objects.lock().unwrap();
    objects.insert(
        V2_NAMESPACE_DESCRIPTOR_PATH.into(),
        serde_json::to_vec(&CloudNamespaceDescriptor::with_library_id(LIBRARY_ID)).unwrap(),
    );
    objects.insert(
        CLOUD_MANIFEST_PATH.into(),
        serde_json::to_vec(&CloudManifest::default()).unwrap(),
    );
    objects.insert(
        SHARED_LIBRARY_PATH.into(),
        serde_json::to_vec(cloud).unwrap(),
    );
    drop(objects);
    transport
}

fn profile(local: &SharedLibrary) -> DeviceProfile {
    let config = crate::config::Config {
        games: local
            .games
            .iter()
            .map(|game| crate::backup::Game {
                name: game.name.clone(),
                storage_key: game.storage_key.clone(),
                save_paths: Vec::new(),
                game_paths: HashMap::new(),
                next_save_unit_id: game.next_save_unit_id,
                cloud_sync_enabled: false,
                auto_backup: None,
                ludusavi_meta: None,
                device_bindings: HashMap::new(),
            })
            .collect(),
        ..Default::default()
    };
    let owners = ConfigurationOwners::from_legacy(&config, &"device".into());
    owners.device_profiles["device"].clone()
}

#[tokio::test]
async fn review_is_read_only_and_classifies_all_local_games() {
    let mut conflict = game("conflict", "Conflict", 1);
    let cloud_conflict = conflict.clone();
    conflict.name = "Local Conflict".into();
    let local = library(vec![
        game("same", "Same", 1),
        game("local", "Local", 1),
        game("duplicate", "Duplicate", 1),
        conflict,
    ]);
    let cloud = library(vec![
        game("same", "Same", 1),
        game("remote-duplicate", "Duplicate", 1),
        cloud_conflict,
    ]);
    let join = CloudLibraryJoin::with_transport(transport(&cloud), 2);

    let review = join.review(&local).await.unwrap();

    assert_eq!(review.items.len(), 4);
    assert_eq!(
        review
            .items
            .iter()
            .map(|item| item.classification)
            .collect::<Vec<_>>(),
        vec![
            GameJoinClassification::GameDefinitionConflict,
            GameJoinClassification::PossibleDuplicate,
            GameJoinClassification::LocalOnly,
            GameJoinClassification::Same,
        ]
    );
    assert!(join.transport.writes.lock().unwrap().is_empty());
}

#[tokio::test]
async fn join_adds_and_replaces_whole_games_then_publishes_profile() {
    let local_only = game("local", "Local", 1);
    let replacement = game("conflict", "Local Conflict", 2);
    let mut cloud_conflict = game("conflict", "Cloud Conflict", 1);
    cloud_conflict.snapshot_retention = Some(SharedSnapshotRetentionPolicy {
        automatic_snapshots_per_branch: 4,
    });
    let local = library(vec![local_only.clone(), replacement.clone()]);
    let cloud = library(vec![cloud_conflict.clone(), game("remote", "Remote", 1)]);
    let review = build_review(&local, &cloud).unwrap();
    let decisions = review
        .items
        .iter()
        .map(|item| JoinGameDecision {
            local_game_id: item.local_game_id.clone(),
            local_fingerprint: item.local_fingerprint.clone(),
            cloud_fingerprint: item.cloud_fingerprint.clone(),
            action: if item.local_game_id == "local" {
                JoinGameAction::AddLocal
            } else {
                JoinGameAction::ReplaceCloud
            },
        })
        .collect::<Vec<_>>();
    let join = CloudLibraryJoin::with_transport(transport(&cloud), 2);

    let result = join
        .join(&local, &profile(&local), &decisions, true)
        .await
        .unwrap();
    assert_eq!(result.library_id, LIBRARY_ID);
    let accepted = result.shared_library;

    assert!(accepted.games.contains(&local_only));
    let accepted_replacement = accepted
        .games
        .iter()
        .find(|game| game.storage_key == "conflict")
        .unwrap();
    assert_eq!(accepted_replacement.name, replacement.name);
    assert_eq!(accepted_replacement.save_units, replacement.save_units);
    assert_eq!(
        accepted_replacement.snapshot_retention,
        cloud_conflict.snapshot_retention
    );
    assert!(
        accepted
            .games
            .iter()
            .any(|game| game.storage_key == "remote")
    );
    assert_eq!(
        join.transport.writes.lock().unwrap().as_slice(),
        [SHARED_LIBRARY_PATH, device_profile_path("device").as_str()]
    );
}

#[tokio::test]
async fn stale_replacement_does_not_write() {
    let local = library(vec![game("game", "Local", 2)]);
    let reviewed_cloud = library(vec![game("game", "Cloud", 1)]);
    let review = build_review(&local, &reviewed_cloud).unwrap();
    let decision = JoinGameDecision {
        local_game_id: "game".into(),
        local_fingerprint: review.items[0].local_fingerprint.clone(),
        cloud_fingerprint: review.items[0].cloud_fingerprint.clone(),
        action: JoinGameAction::ReplaceCloud,
    };
    let changed_cloud = library(vec![game("game", "Changed Again", 3)]);
    let join = CloudLibraryJoin::with_transport(transport(&changed_cloud), 2);

    assert!(matches!(
        join.join(&local, &profile(&local), &[decision], true)
            .await,
        Err(CloudLibraryJoinError::TargetChanged(name)) if name == "Local"
    ));
    assert!(join.transport.writes.lock().unwrap().is_empty());
}
