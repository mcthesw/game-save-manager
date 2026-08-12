use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use opendal::Operator;
use rgsm_core::backup::{ArchiveFormat, CreatedBy, Game, GameSnapshots, Snapshot, archive_path};
use rgsm_core::cloud_sync::v2::{
    CLOUD_MANIFEST_PATH, CloudLibraryBootstrap, CloudManifest, CloudManifestRepository,
    DeviceProfileRepository, SharedLibraryRepository, SnapshotSyncCoordinator, cloud_archive_path,
};
use rgsm_core::cloud_sync::{Backend, CloudSyncSessionConfig};
use rgsm_core::config::{
    Config, ConfigurationOwners, DeviceProfile, SharedLibrary, V2_CONFIG_SCHEMA_VERSION,
};
use rgsm_core::device::DeviceId;
use tokio_util::sync::CancellationToken;

pub const MAX_ATTEMPTS: usize = 2;

pub struct FsCloudFixture {
    root: temp_dir::TempDir,
}

impl FsCloudFixture {
    pub fn new() -> Self {
        Self {
            root: temp_dir::TempDir::new().expect("temporary cloud root should initialize"),
        }
    }

    pub fn root(&self) -> &Path {
        self.root.path()
    }

    pub fn new_operator(&self) -> Operator {
        CloudSyncSessionConfig {
            root_path: self.root.path().to_string_lossy().into_owned(),
            max_concurrency: 2,
            backend: Backend::Fs,
        }
        .get_op()
        .expect("Fs cloud operator should initialize")
    }
}

pub struct DeviceFixture {
    pub id: DeviceId,
    pub archive_root: PathBuf,
    pub progress_path: PathBuf,
    _root: temp_dir::TempDir,
}

impl DeviceFixture {
    pub fn new(id: &str) -> Self {
        let root = temp_dir::TempDir::new().expect("temporary device root should initialize");
        Self {
            id: id.to_string(),
            archive_root: root.path().join("archives"),
            progress_path: root.path().join("materialization-progress.json"),
            _root: root,
        }
    }

    pub fn empty_library_and_profile(&self) -> (SharedLibrary, DeviceProfile) {
        let owners = ConfigurationOwners::from_legacy(&Config::default(), &self.id);
        let profile = owners
            .device_profiles
            .get(&self.id)
            .expect("current device profile should exist")
            .clone();
        (
            SharedLibrary {
                schema_version: V2_CONFIG_SCHEMA_VERSION,
                games: Vec::new(),
            },
            profile,
        )
    }

    pub fn library_and_profile_with_game(
        &self,
        game_id: &str,
        game_name: &str,
    ) -> (SharedLibrary, DeviceProfile) {
        let config = Config {
            backup_path: self.archive_root.to_string_lossy().into_owned(),
            games: vec![Game {
                name: game_name.to_string(),
                storage_key: game_id.to_string(),
                save_paths: Vec::new(),
                game_paths: HashMap::new(),
                next_save_unit_id: 0,
                cloud_sync_enabled: true,
                auto_backup: None,
                ludusavi_meta: None,
                device_bindings: HashMap::new(),
            }],
            ..Config::default()
        };
        let owners = ConfigurationOwners::from_legacy(&config, &self.id);
        let profile = owners
            .device_profiles
            .get(&self.id)
            .expect("current device profile should exist")
            .clone();
        (owners.shared_library, profile)
    }

    pub fn write_archive(&self, game_id: &str, snapshot: &Snapshot, bytes: &[u8]) -> PathBuf {
        let path = archive_path(
            &self.archive_root.join(game_id),
            &snapshot.date,
            snapshot.archive_format,
        );
        std::fs::create_dir_all(path.parent().expect("archive path should have a parent"))
            .expect("archive directory should initialize");
        std::fs::write(&path, bytes).expect("archive bytes should be writable");
        path
    }

    pub fn snapshots(&self, game_name: &str, backups: Vec<Snapshot>, head: &str) -> GameSnapshots {
        let mut snapshots = GameSnapshots::new(game_name);
        snapshots.backups = backups;
        snapshots.set_head_for_device(self.id.clone(), Some(head.to_string()));
        snapshots
    }

    pub fn archive_path(&self, game_id: &str, snapshot: &Snapshot) -> PathBuf {
        archive_path(
            &self.archive_root.join(game_id),
            &snapshot.date,
            snapshot.archive_format,
        )
    }
}

pub async fn bootstrap_game(
    cloud: &FsCloudFixture,
    device_a: &DeviceFixture,
    device_b: Option<&DeviceFixture>,
    game_id: &str,
    game_name: &str,
) {
    let (empty_library, empty_profile) = device_a.empty_library_and_profile();
    CloudLibraryBootstrap::new(cloud.new_operator(), MAX_ATTEMPTS)
        .create_empty(&empty_library, &empty_profile)
        .await
        .expect("empty Fs root should bootstrap");

    let (library, profile_a) = device_a.library_and_profile_with_game(game_id, game_name);
    SharedLibraryRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
        .compare_replace(&empty_library, &library)
        .await
        .expect("Shared Library should publish");
    DeviceProfileRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
        .publish(&device_a.id, &profile_a)
        .await
        .expect("device A profile should publish");

    if let Some(device_b) = device_b {
        let (_, profile_b) = device_b.library_and_profile_with_game(game_id, game_name);
        DeviceProfileRepository::new(cloud.new_operator(), MAX_ATTEMPTS)
            .publish(&device_b.id, &profile_b)
            .await
            .expect("device B profile should publish");
    }
}

pub async fn reconcile_game(
    cloud: &FsCloudFixture,
    device: &DeviceFixture,
    game_id: &str,
    snapshots: &GameSnapshots,
) {
    SnapshotSyncCoordinator::new(
        cloud.new_operator(),
        device.archive_root.clone(),
        device.id.clone(),
        device.progress_path.clone(),
        MAX_ATTEMPTS,
    )
    .reconcile_game(
        game_id,
        snapshots,
        0,
        &no_baseline(),
        &CancellationToken::new(),
    )
    .await
    .expect("Snapshot reconciliation should succeed");
}

pub async fn load_manifest(cloud: &FsCloudFixture) -> CloudManifest {
    CloudManifestRepository::new(cloud.new_operator(), CLOUD_MANIFEST_PATH, MAX_ATTEMPTS)
        .load()
        .await
        .expect("Cloud Manifest should load through a fresh Operator")
}

pub fn cloud_archive(game_id: &str, snapshot: &Snapshot) -> String {
    cloud_archive_path(game_id, &snapshot.date, snapshot.archive_format)
        .expect("cloud archive path should be valid")
}

pub fn read_progress(device: &DeviceFixture) -> serde_json::Value {
    serde_json::from_slice(
        &std::fs::read(&device.progress_path).expect("materialization progress should persist"),
    )
    .expect("materialization progress should be valid JSON")
}

pub fn snapshot(id: &str, parent: Option<&str>, device_id: &str, size: usize) -> Snapshot {
    Snapshot {
        date: id.to_string(),
        describe: format!("Snapshot {id}"),
        path: format!("{id}.zip"),
        archive_format: ArchiveFormat::Zip,
        size: size as u64,
        parent: parent.map(str::to_string),
        archive_hash: None,
        device_id: Some(device_id.to_string()),
        created_by: CreatedBy::Manual,
    }
}

pub fn no_baseline() -> BTreeSet<String> {
    BTreeSet::new()
}
