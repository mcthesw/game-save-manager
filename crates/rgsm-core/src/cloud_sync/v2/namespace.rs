use std::sync::Arc;

use async_trait::async_trait;
use futures_util::TryStreamExt;
use opendal::{ErrorKind, Operator};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::super::V1_CONFIG_PATH;
#[cfg(test)]
use super::super::V1_SAVE_DATA_PREFIX;
use super::{CloudManifest, ManifestError};
use crate::config::{Config, OwnershipError, SharedLibrary};
use crate::device::encode_device_id;
use crate::{backup::ArchiveFormat, cloud_sync::transfer::path_to_remote_key};

pub const V2_NAMESPACE_SCHEMA_VERSION: u32 = 2;
pub const V2_NAMESPACE_PREFIX: &str = "v2/";
pub const V2_NAMESPACE_DESCRIPTOR_PATH: &str = "v2/namespace.json";
pub const SHARED_LIBRARY_PATH: &str = "v2/shared-library.json";
pub const V2_DEVICE_PROFILES_PREFIX: &str = "v2/device-profiles/";
pub const CLOUD_MANIFEST_PATH: &str = "v2/cloud-manifest.json";
pub const CLOUD_ARCHIVES_PREFIX: &str = "v2/archives/";
const CLASSIFICATION_ENTRY_SAMPLE_LIMIT: usize = 8;

pub fn device_profile_path(device_id: &str) -> String {
    format!(
        "{V2_DEVICE_PROFILES_PREFIX}{}.json",
        encode_device_id(device_id)
    )
}

pub fn cloud_archive_path(
    game_id: &str,
    snapshot_id: &str,
    format: ArchiveFormat,
) -> Result<String, crate::preclude::BackendError> {
    path_to_remote_key(
        &std::path::PathBuf::from(CLOUD_ARCHIVES_PREFIX)
            .join(game_id)
            .join(format!("{snapshot_id}.{}", format.extension())),
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudNamespaceDescriptor {
    pub schema_version: u32,
}

impl Default for CloudNamespaceDescriptor {
    fn default() -> Self {
        Self {
            schema_version: V2_NAMESPACE_SCHEMA_VERSION,
        }
    }
}

#[derive(Debug)]
pub enum CloudNamespaceClassification {
    SupportedV2 {
        descriptor: CloudNamespaceDescriptor,
        shared_library: SharedLibrary,
        manifest: CloudManifest,
    },
    V1Only {
        config: Box<Config>,
    },
    Empty,
}

#[async_trait]
pub trait NamespaceTransport: Send + Sync {
    async fn read(&self, path: &str) -> Result<Option<Vec<u8>>, opendal::Error>;
    async fn list_sample(&self, prefix: &str, limit: usize) -> Result<Vec<String>, opendal::Error>;
}

#[async_trait]
impl<T: NamespaceTransport + ?Sized> NamespaceTransport for Arc<T> {
    async fn read(&self, path: &str) -> Result<Option<Vec<u8>>, opendal::Error> {
        self.as_ref().read(path).await
    }

    async fn list_sample(&self, prefix: &str, limit: usize) -> Result<Vec<String>, opendal::Error> {
        self.as_ref().list_sample(prefix, limit).await
    }
}

pub struct OpenDalNamespaceTransport {
    pub(super) operator: Operator,
}

impl OpenDalNamespaceTransport {
    pub fn new(operator: Operator) -> Self {
        Self { operator }
    }
}

#[async_trait]
impl NamespaceTransport for OpenDalNamespaceTransport {
    async fn read(&self, path: &str) -> Result<Option<Vec<u8>>, opendal::Error> {
        match self.operator.read(path).await {
            Ok(bytes) => Ok(Some(bytes.to_vec())),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn list_sample(&self, prefix: &str, limit: usize) -> Result<Vec<String>, opendal::Error> {
        let mut lister = self.operator.lister(prefix).await?;
        let mut paths = Vec::with_capacity(limit);
        while paths.len() < limit {
            let Some(entry) = lister.try_next().await? else {
                break;
            };
            paths.push(entry.path().to_string());
        }
        Ok(paths)
    }
}

pub struct CloudNamespaceClassifier<T: NamespaceTransport> {
    transport: T,
}

impl CloudNamespaceClassifier<OpenDalNamespaceTransport> {
    pub fn new(operator: Operator) -> Self {
        Self::with_transport(OpenDalNamespaceTransport::new(operator))
    }
}

impl<T: NamespaceTransport> CloudNamespaceClassifier<T> {
    pub fn with_transport(transport: T) -> Self {
        Self { transport }
    }

    /// Classify without writing, deleting, or interpreting provider errors as
    /// absence. Empty requires a successful listing of the entire root.
    pub async fn classify(&self) -> Result<CloudNamespaceClassification, CloudNamespaceError> {
        if let Some(bytes) = self.transport.read(V2_NAMESPACE_DESCRIPTOR_PATH).await? {
            return self.classify_v2(&bytes).await;
        }

        let v2_entries = self
            .transport
            .list_sample(V2_NAMESPACE_PREFIX, CLASSIFICATION_ENTRY_SAMPLE_LIMIT)
            .await?;
        if !v2_entries.is_empty() {
            return Err(CloudNamespaceError::PartialV2(v2_entries));
        }

        if let Some(bytes) = self.transport.read(V1_CONFIG_PATH).await? {
            let config = parse_json(V1_CONFIG_PATH, &bytes)?;
            return Ok(CloudNamespaceClassification::V1Only {
                config: Box::new(config),
            });
        }

        let root_entries = self
            .transport
            .list_sample(".", CLASSIFICATION_ENTRY_SAMPLE_LIMIT)
            .await?;
        if root_entries.is_empty() {
            Ok(CloudNamespaceClassification::Empty)
        } else {
            Err(CloudNamespaceError::UnrecognizedRoot(root_entries))
        }
    }

    async fn classify_v2(
        &self,
        descriptor_bytes: &[u8],
    ) -> Result<CloudNamespaceClassification, CloudNamespaceError> {
        let descriptor: CloudNamespaceDescriptor =
            parse_json(V2_NAMESPACE_DESCRIPTOR_PATH, descriptor_bytes)?;
        if descriptor.schema_version != V2_NAMESPACE_SCHEMA_VERSION {
            return Err(CloudNamespaceError::UnsupportedSchema {
                object: V2_NAMESPACE_DESCRIPTOR_PATH,
                found: descriptor.schema_version,
            });
        }

        let shared_bytes = self.required_object(SHARED_LIBRARY_PATH).await?;
        let shared_library: SharedLibrary = parse_json(SHARED_LIBRARY_PATH, &shared_bytes)?;
        shared_library.validate()?;

        let manifest_bytes = self.required_object(CLOUD_MANIFEST_PATH).await?;
        let manifest: CloudManifest = parse_json(CLOUD_MANIFEST_PATH, &manifest_bytes)?;
        manifest.validate()?;

        Ok(CloudNamespaceClassification::SupportedV2 {
            descriptor,
            shared_library,
            manifest,
        })
    }

    async fn required_object(&self, path: &'static str) -> Result<Vec<u8>, CloudNamespaceError> {
        self.transport
            .read(path)
            .await?
            .ok_or(CloudNamespaceError::MissingRequiredObject(path))
    }
}

fn parse_json<T: for<'de> Deserialize<'de>>(
    path: &'static str,
    bytes: &[u8],
) -> Result<T, CloudNamespaceError> {
    serde_json::from_slice(bytes)
        .map_err(|source| CloudNamespaceError::MalformedObject { path, source })
}

#[derive(Debug, Error)]
pub enum CloudNamespaceError {
    #[error("Cloud namespace transport error: {0}")]
    Transport(#[from] opendal::Error),
    #[error("Malformed cloud object {path}: {source}")]
    MalformedObject {
        path: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("Unsupported schema {found} in {object}")]
    UnsupportedSchema { object: &'static str, found: u32 },
    #[error("V2 namespace is missing required object {0}")]
    MissingRequiredObject(&'static str),
    #[error("V2 namespace descriptor is absent but V2 objects remain: {0:?}")]
    PartialV2(Vec<String>),
    #[error("Cloud root is non-empty but is not recognized: {0:?}")]
    UnrecognizedRoot(Vec<String>),
    #[error(transparent)]
    SharedLibrary(#[from] OwnershipError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use opendal::services;

    use super::*;
    use crate::config::V2_CONFIG_SCHEMA_VERSION;

    #[derive(Default)]
    struct FakeTransport {
        objects: BTreeMap<String, Vec<u8>>,
        fail_read: Option<String>,
        fail_list: Option<String>,
    }

    #[async_trait]
    impl NamespaceTransport for FakeTransport {
        async fn read(&self, path: &str) -> Result<Option<Vec<u8>>, opendal::Error> {
            if self.fail_read.as_deref() == Some(path) {
                return Err(opendal::Error::new(
                    ErrorKind::Unexpected,
                    "injected read failure",
                ));
            }
            Ok(self.objects.get(path).cloned())
        }

        async fn list_sample(
            &self,
            prefix: &str,
            limit: usize,
        ) -> Result<Vec<String>, opendal::Error> {
            if self.fail_list.as_deref() == Some(prefix) {
                return Err(opendal::Error::new(
                    ErrorKind::Unexpected,
                    "injected list failure",
                ));
            }
            Ok(self
                .objects
                .keys()
                .filter(|path| prefix == "." || path.starts_with(prefix))
                .take(limit)
                .cloned()
                .collect())
        }
    }

    fn shared_library() -> SharedLibrary {
        SharedLibrary {
            schema_version: V2_CONFIG_SCHEMA_VERSION,
            games: Vec::new(),
        }
    }

    fn complete_v2() -> FakeTransport {
        let mut transport = FakeTransport::default();
        transport.objects.insert(
            V2_NAMESPACE_DESCRIPTOR_PATH.into(),
            serde_json::to_vec(&CloudNamespaceDescriptor::default()).unwrap(),
        );
        transport.objects.insert(
            SHARED_LIBRARY_PATH.into(),
            serde_json::to_vec(&shared_library()).unwrap(),
        );
        transport.objects.insert(
            CLOUD_MANIFEST_PATH.into(),
            serde_json::to_vec(&CloudManifest::default()).unwrap(),
        );
        transport
    }

    #[tokio::test]
    async fn complete_v2_v1_and_empty_roots_are_distinct() {
        let v2 = CloudNamespaceClassifier::with_transport(complete_v2())
            .classify()
            .await
            .unwrap();
        assert!(matches!(
            v2,
            CloudNamespaceClassification::SupportedV2 { .. }
        ));

        let mut legacy = FakeTransport::default();
        legacy.objects.insert(
            V1_CONFIG_PATH.into(),
            serde_json::to_vec(&Config::default()).unwrap(),
        );
        assert!(matches!(
            CloudNamespaceClassifier::with_transport(legacy)
                .classify()
                .await
                .unwrap(),
            CloudNamespaceClassification::V1Only { .. }
        ));

        assert!(matches!(
            CloudNamespaceClassifier::with_transport(FakeTransport::default())
                .classify()
                .await
                .unwrap(),
            CloudNamespaceClassification::Empty
        ));
    }

    #[tokio::test]
    async fn v2_required_objects_and_schema_fail_closed() {
        let mut missing = complete_v2();
        missing.objects.remove(SHARED_LIBRARY_PATH);
        assert!(matches!(
            CloudNamespaceClassifier::with_transport(missing)
                .classify()
                .await,
            Err(CloudNamespaceError::MissingRequiredObject(
                SHARED_LIBRARY_PATH
            ))
        ));

        let mut unsupported = complete_v2();
        unsupported.objects.insert(
            V2_NAMESPACE_DESCRIPTOR_PATH.into(),
            serde_json::to_vec(&CloudNamespaceDescriptor {
                schema_version: V2_NAMESPACE_SCHEMA_VERSION + 1,
            })
            .unwrap(),
        );
        assert!(matches!(
            CloudNamespaceClassifier::with_transport(unsupported)
                .classify()
                .await,
            Err(CloudNamespaceError::UnsupportedSchema { .. })
        ));

        let mut corrupt = complete_v2();
        corrupt
            .objects
            .insert(CLOUD_MANIFEST_PATH.into(), b"{broken".to_vec());
        assert!(matches!(
            CloudNamespaceClassifier::with_transport(corrupt)
                .classify()
                .await,
            Err(CloudNamespaceError::MalformedObject {
                path: CLOUD_MANIFEST_PATH,
                ..
            })
        ));

        let mut invalid_library = complete_v2();
        let mut library = shared_library();
        library.games.push(crate::config::SharedGame {
            name: "Game".into(),
            storage_key: String::new(),
            save_units: Vec::new(),
            next_save_unit_id: 0,
            ludusavi_meta: None,
        });
        invalid_library.objects.insert(
            SHARED_LIBRARY_PATH.into(),
            serde_json::to_vec(&library).unwrap(),
        );
        assert!(matches!(
            CloudNamespaceClassifier::with_transport(invalid_library)
                .classify()
                .await,
            Err(CloudNamespaceError::SharedLibrary(
                OwnershipError::EmptySharedGameId
            ))
        ));
    }

    #[tokio::test]
    async fn partial_unknown_and_provider_failures_never_become_empty() {
        let mut partial = FakeTransport::default();
        partial
            .objects
            .insert(SHARED_LIBRARY_PATH.into(), b"{}".to_vec());
        assert!(matches!(
            CloudNamespaceClassifier::with_transport(partial)
                .classify()
                .await,
            Err(CloudNamespaceError::PartialV2(_))
        ));

        let mut unknown = FakeTransport::default();
        unknown.objects.insert("other/file".into(), Vec::new());
        assert!(matches!(
            CloudNamespaceClassifier::with_transport(unknown)
                .classify()
                .await,
            Err(CloudNamespaceError::UnrecognizedRoot(_))
        ));

        let failed = FakeTransport {
            fail_list: Some(V2_NAMESPACE_PREFIX.into()),
            ..FakeTransport::default()
        };
        assert!(matches!(
            CloudNamespaceClassifier::with_transport(failed)
                .classify()
                .await,
            Err(CloudNamespaceError::Transport(_))
        ));

        let failed_descriptor_read = FakeTransport {
            fail_read: Some(V2_NAMESPACE_DESCRIPTOR_PATH.into()),
            ..FakeTransport::default()
        };
        assert!(matches!(
            CloudNamespaceClassifier::with_transport(failed_descriptor_read)
                .classify()
                .await,
            Err(CloudNamespaceError::Transport(_))
        ));

        let failed_v1_read = FakeTransport {
            fail_read: Some(V1_CONFIG_PATH.into()),
            ..FakeTransport::default()
        };
        assert!(matches!(
            CloudNamespaceClassifier::with_transport(failed_v1_read)
                .classify()
                .await,
            Err(CloudNamespaceError::Transport(_))
        ));

        let failed_root_list = FakeTransport {
            fail_list: Some(".".into()),
            ..FakeTransport::default()
        };
        assert!(matches!(
            CloudNamespaceClassifier::with_transport(failed_root_list)
                .classify()
                .await,
            Err(CloudNamespaceError::Transport(_))
        ));

        let mut corrupt_legacy = FakeTransport::default();
        corrupt_legacy
            .objects
            .insert(V1_CONFIG_PATH.into(), b"{broken".to_vec());
        assert!(matches!(
            CloudNamespaceClassifier::with_transport(corrupt_legacy)
                .classify()
                .await,
            Err(CloudNamespaceError::MalformedObject {
                path: V1_CONFIG_PATH,
                ..
            })
        ));
    }

    #[test]
    fn v1_and_v2_paths_are_disjoint() {
        for v2_path in [
            V2_NAMESPACE_DESCRIPTOR_PATH,
            SHARED_LIBRARY_PATH,
            V2_DEVICE_PROFILES_PREFIX,
            CLOUD_MANIFEST_PATH,
            CLOUD_ARCHIVES_PREFIX,
        ] {
            assert!(v2_path.starts_with(V2_NAMESPACE_PREFIX));
            assert_ne!(v2_path, V1_CONFIG_PATH);
            assert!(!v2_path.starts_with(V1_SAVE_DATA_PREFIX));
        }
    }

    #[tokio::test]
    async fn opendal_adapter_classifies_current_v1_object_path() {
        let operator = Operator::new(services::Memory::default()).unwrap().finish();
        operator
            .write(
                V1_CONFIG_PATH,
                serde_json::to_vec(&Config::default()).unwrap(),
            )
            .await
            .unwrap();

        assert!(matches!(
            CloudNamespaceClassifier::new(operator)
                .classify()
                .await
                .unwrap(),
            CloudNamespaceClassification::V1Only { .. }
        ));
    }
}
