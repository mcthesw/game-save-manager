use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::super::{CloudManifest, SnapshotNode};
use super::*;
use crate::backup::{GameSnapshots, PendingDescription};
use async_trait::async_trait;

#[derive(Clone)]
struct Transport {
    bytes: Arc<Mutex<Vec<u8>>>,
    offline: Arc<AtomicBool>,
}

#[async_trait]
impl ManifestTransport for Transport {
    async fn read(&self) -> Result<Option<Vec<u8>>, opendal::Error> {
        Ok(Some(self.bytes.lock().unwrap().clone()))
    }
    async fn write(&self, bytes: &[u8]) -> Result<(), opendal::Error> {
        if self.offline.load(Ordering::SeqCst) {
            return Err(opendal::Error::new(
                opendal::ErrorKind::Unexpected,
                "offline",
            ));
        }
        *self.bytes.lock().unwrap() = bytes.to_vec();
        Ok(())
    }
}

fn fixture() -> (Transport, GameSnapshots) {
    let mut remote = CloudManifest::default();
    let mut local = GameSnapshots::new("game");
    for id in ["one", "two"] {
        let snapshot = serde_json::from_value(serde_json::json!({
            "date": id, "describe": format!("original {id}"), "path": "never-open-this.zip"
        }))
        .unwrap();
        remote
            .game_mut("game")
            .upsert_live(SnapshotNode::from_snapshot(&snapshot, None))
            .unwrap();
        local.backups.push(snapshot);
    }
    remote
        .game_mut("game")
        .set_head("device".into(), "one".into());
    (
        Transport {
            bytes: Arc::new(Mutex::new(serde_json::to_vec(&remote).unwrap())),
            offline: Arc::new(AtomicBool::new(false)),
        },
        local,
    )
}

fn edit(local: &mut GameSnapshots, library: &str, text: &str) {
    local.backups[0].describe = text.into();
    local.pending_descriptions.insert(
        "one".into(),
        PendingDescription {
            library_id: library.into(),
            description: text.into(),
        },
    );
}

#[tokio::test]
async fn only_explicit_edits_publish_and_remote_text_updates_existing_copies() {
    let (transport, mut a) = fixture();
    let mut b = a.clone();
    let repo = CloudManifestRepository::with_transport(transport, 1);
    let before = repo.load().await.unwrap();
    edit(&mut a, "library", "edited");
    // Stale unrelated descriptions must never be published along with an edit.
    a.backups[1].describe = "stale unrelated text".into();
    sync_descriptions(&repo, "library", "game", &mut a)
        .await
        .unwrap();
    let mut expected = before;
    expected.revision += 1;
    expected
        .games
        .get_mut("game")
        .unwrap()
        .snapshots
        .get_mut("one")
        .unwrap()
        .description = "edited".into();
    assert_eq!(repo.load().await.unwrap(), expected);
    assert!(a.pending_descriptions.is_empty());
    sync_descriptions(&repo, "library", "game", &mut b)
        .await
        .unwrap();
    assert_eq!(b.backups[0].describe, "edited");
    assert_eq!(repo.load().await.unwrap(), expected); // Refresh is not a write.
    edit(&mut b, "library", "");
    sync_descriptions(&repo, "library", "game", &mut b)
        .await
        .unwrap();
    sync_descriptions(&repo, "library", "game", &mut a)
        .await
        .unwrap();
    assert_eq!(a.backups[0].describe, "");
    assert_eq!(a.backups[1].describe, "original two");
}

#[tokio::test]
async fn failed_write_retains_edit_across_reload_and_retries() {
    let (transport, mut local) = fixture();
    let repo = CloudManifestRepository::with_transport(transport.clone(), 1);
    edit(&mut local, "library", "offline edit");
    transport.offline.store(true, Ordering::SeqCst);
    let before = local.clone();
    assert!(
        sync_descriptions(&repo, "library", "game", &mut local)
            .await
            .is_err()
    );
    assert_eq!(local, before);
    let mut reloaded = serde_json::from_slice(&serde_json::to_vec(&local).unwrap()).unwrap();
    transport.offline.store(false, Ordering::SeqCst);
    sync_descriptions(&repo, "library", "game", &mut reloaded)
        .await
        .unwrap();
    assert!(reloaded.pending_descriptions.is_empty());
    assert_eq!(
        repo.load().await.unwrap().games["game"].snapshots["one"].description,
        "offline edit"
    );
}

#[tokio::test]
async fn pending_edits_do_not_cross_libraries_or_resurrect_deleted_snapshots() {
    let (transport, mut local) = fixture();
    let repo = CloudManifestRepository::with_transport(transport, 1);
    let before = repo.load().await.unwrap();
    edit(&mut local, "old-library", "must not publish");
    sync_descriptions(&repo, "new-library", "game", &mut local)
        .await
        .unwrap();
    assert_eq!(repo.load().await.unwrap(), before);
    assert!(local.pending_descriptions.is_empty());
    repo.mutate(|m| {
        m.game_mut("game")
            .begin_deletion("one", "device", crate::cloud_sync::v2::DeletionKind::User)
            .map(|_| ())
    })
    .await
    .unwrap();
    let deleted = repo.load().await.unwrap();
    edit(&mut local, "new-library", "deleted edit");
    sync_descriptions(&repo, "new-library", "game", &mut local)
        .await
        .unwrap();
    assert_eq!(repo.load().await.unwrap(), deleted);
    assert!(local.pending_descriptions.is_empty());
}
