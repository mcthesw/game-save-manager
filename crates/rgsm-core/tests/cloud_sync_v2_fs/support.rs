use std::path::Path;

use opendal::Operator;
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
}

impl DeviceFixture {
    pub fn new(id: &str) -> Self {
        Self { id: id.to_string() }
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
}
