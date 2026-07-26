use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use opendal::Operator;
use rgsm_core::backup::{ArchiveFormat, CreatedBy, Game, GameSnapshots, Snapshot, archive_path};
use rgsm_core::cloud_sync::{Backend, CloudSyncSessionConfig};
use rgsm_core::config::{
    Config, ConfigurationOwners, DeviceProfile, SharedLibrary, V2_CONFIG_SCHEMA_VERSION,
};
use rgsm_core::device::DeviceId;

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
