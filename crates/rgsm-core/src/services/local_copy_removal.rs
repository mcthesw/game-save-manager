use std::{collections::BTreeSet, time::Duration};

use anyhow::anyhow;
use serde::Serialize;
use specta::Type;

use crate::backup::{GameSnapshots, snapshot_archive_path};
use crate::cloud_sync::v2::{
    CLOUD_MANIFEST_PATH, CloudManifestRepository, GameManifest, SnapshotState,
};
use crate::config::{cloud_bootstrap_inputs, get_config, resolve_backup_path};

use super::{ServiceContext, cloud_library_target::bound_v2_operator};

/// The check only informs a confirmation, so an unreachable cloud must not
/// keep the player waiting on retries.
const CLOUD_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type, utoipa::ToSchema)]
pub struct LocalCopyRemovalPreview {
    /// False when the cloud could not be reached, in which case every local
    /// archive is counted because none can be shown to exist elsewhere.
    pub cloud_checked: bool,
    /// Local archives that the cloud does not hold.
    pub only_on_this_device: usize,
}

impl ServiceContext {
    pub async fn local_copy_removal_preview(
        &self,
        identity: &str,
    ) -> anyhow::Result<LocalCopyRemovalPreview> {
        let config = get_config()?;
        let index = config
            .position_game_by_identity(identity)
            .ok_or_else(|| anyhow!("Game '{}' not found", identity))?;
        let game = &config.games[index];
        let snapshots = match game.get_game_snapshots_info() {
            Ok(snapshots) => snapshots,
            Err(crate::preclude::BackupError::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                GameSnapshots::new(game.name.clone())
            }
            Err(error) => return Err(error.into()),
        };
        let game_dir =
            resolve_backup_path(&config.backup_path).join(game.backup_dir_name().as_ref());
        let local = snapshots
            .backups
            .iter()
            .filter(|snapshot| snapshot_archive_path(&game_dir, snapshot).is_file())
            .map(|snapshot| snapshot.date.clone())
            .collect::<Vec<_>>();

        let manifest = tokio::time::timeout(CLOUD_CHECK_TIMEOUT, load_game_manifest(identity))
            .await
            .ok()
            .flatten();
        Ok(match manifest {
            Some(game) => LocalCopyRemovalPreview {
                cloud_checked: true,
                only_on_this_device: archives_missing_from_cloud(&local, game.as_ref()),
            },
            None => LocalCopyRemovalPreview {
                cloud_checked: false,
                only_on_this_device: local.len(),
            },
        })
    }
}

/// `None` means the cloud catalog could not be read; `Some(None)` means it was
/// read and does not know this game.
async fn load_game_manifest(game_id: &str) -> Option<Option<GameManifest>> {
    let (_, _, local_state) = cloud_bootstrap_inputs().ok()?;
    let operator = bound_v2_operator(&local_state).await.ok()?;
    let mut manifest = CloudManifestRepository::new(operator, CLOUD_MANIFEST_PATH, 1)
        .load()
        .await
        .ok()?;
    Some(manifest.games.remove(game_id))
}

fn archives_missing_from_cloud(local: &[String], game: Option<&GameManifest>) -> usize {
    let held = game
        .map(|game| {
            game.snapshots
                .values()
                .filter(|node| {
                    matches!(&node.state, SnapshotState::Live(live) if live.cloud_archive_verified)
                })
                .map(|node| node.snapshot_id.as_str())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    local
        .iter()
        .filter(|id| !held.contains(id.as_str()))
        .count()
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc};

    use crate::backup::{ArchiveFormat, CreatedBy, Game, Snapshot};
    use crate::cloud_sync::v2::SnapshotNode;
    use crate::config::{Config, ConfigTestStateGuard, set_config_local};
    use crate::hooks::HookPipeline;

    use super::*;

    #[test]
    fn archives_the_cloud_does_not_hold_are_counted() {
        let mut game = GameManifest::new("game");
        for (id, verified) in [("held", true), ("metadata-only", false)] {
            let mut node = SnapshotNode::unavailable(id, None, CreatedBy::Manual);
            if let SnapshotState::Live(live) = &mut node.state {
                live.cloud_archive_verified = verified;
            }
            game.upsert_live(node).unwrap();
        }
        let local = ["held", "metadata-only", "unpublished"].map(String::from);

        assert_eq!(archives_missing_from_cloud(&local, Some(&game)), 2);
        assert_eq!(archives_missing_from_cloud(&local, None), 3);
    }

    #[test]
    fn unreachable_cloud_counts_every_local_archive_and_does_not_fail() {
        let _lock = crate::config::lock_config_test_file();
        let root = temp_dir::TempDir::new().unwrap();
        let game: Game = serde_json::from_value(serde_json::json!({
            "name": "Offline", "storage_key": "offline", "save_paths": []
        }))
        .unwrap();
        let config = Config {
            backup_path: root.path().join("archives").to_string_lossy().into_owned(),
            games: vec![game.clone()],
            ..Config::default()
        };
        let _guard = ConfigTestStateGuard::replace_with(&config).unwrap();
        set_config_local(&config).unwrap();
        let directory = root.path().join("archives/offline");
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("a.7z"), "a").unwrap();
        let mut snapshots = GameSnapshots::new("Offline");
        snapshots.backups.push(Snapshot {
            date: "a".into(),
            describe: String::new(),
            path: String::new(),
            archive_name: None,
            archive_format: ArchiveFormat::SevenZ,
            size: 1,
            parent: None,
            archive_hash: None,
            created_at: Some(1),
            device_id: None,
            created_by: CreatedBy::Manual,
        });
        game.set_game_snapshots_info(&snapshots).unwrap();

        let preview = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(
                ServiceContext::new(Arc::new(HookPipeline::new(vec![])))
                    .local_copy_removal_preview("offline"),
            )
            .unwrap();

        assert_eq!(
            preview,
            LocalCopyRemovalPreview {
                cloud_checked: false,
                only_on_this_device: 1,
            }
        );
    }
}
