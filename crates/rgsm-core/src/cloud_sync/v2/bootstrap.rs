use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use thiserror::Error;

use super::{
    CLOUD_MANIFEST_PATH, CloudManifest, CloudNamespaceClassification, CloudNamespaceClassifier,
    CloudNamespaceDescriptor, CloudNamespaceError, DELETION_REGISTRY_PATH, DeletionRegistry,
    ManifestError, NamespaceTransport, OpenDalNamespaceTransport, SHARED_LIBRARY_PATH,
    V2_NAMESPACE_DESCRIPTOR_PATH, device_profile_path,
};
use crate::config::{DeviceProfile, OwnershipError, SharedLibrary};

#[async_trait]
pub trait CloudLibraryTransport: NamespaceTransport {
    async fn write(&self, path: &str, bytes: &[u8]) -> Result<(), opendal::Error>;
}

#[async_trait]
impl CloudLibraryTransport for OpenDalNamespaceTransport {
    async fn write(&self, path: &str, bytes: &[u8]) -> Result<(), opendal::Error> {
        self.operator.write(path, bytes.to_vec()).await?;
        Ok(())
    }
}

pub struct CloudLibraryBootstrap<T: CloudLibraryTransport> {
    transport: Arc<T>,
    max_attempts: usize,
}

impl CloudLibraryBootstrap<OpenDalNamespaceTransport> {
    pub fn new(operator: opendal::Operator, max_attempts: usize) -> Self {
        Self::with_transport(OpenDalNamespaceTransport::new(operator), max_attempts)
    }
}

impl<T: CloudLibraryTransport> CloudLibraryBootstrap<T> {
    pub fn with_transport(transport: T, max_attempts: usize) -> Self {
        Self {
            transport: Arc::new(transport),
            max_attempts: max_attempts.max(1),
        }
    }

    pub async fn inspect(
        &self,
    ) -> Result<CloudNamespaceClassification, CloudLibraryBootstrapError> {
        Ok(
            CloudNamespaceClassifier::with_transport(self.transport.clone())
                .classify()
                .await?,
        )
    }

    /// Create the complete namespace only from a freshly reclassified empty root.
    ///
    /// The descriptor is intentionally written last. A stopped process therefore
    /// leaves either no V2 state, a fail-closed partial namespace, or a complete
    /// namespace; it never advertises an incomplete library as supported.
    pub async fn create_empty(
        &self,
        shared_library: &SharedLibrary,
        device_profile: &DeviceProfile,
    ) -> Result<(), CloudLibraryBootstrapError> {
        match self.inspect().await? {
            CloudNamespaceClassification::Empty => {}
            CloudNamespaceClassification::SupportedV2 { .. } => {
                return Err(CloudLibraryBootstrapError::RootChanged("existing_v2"));
            }
            CloudNamespaceClassification::V1Only { .. } => {
                return Err(CloudLibraryBootstrapError::RootChanged("legacy_v1"));
            }
        }

        shared_library.validate()?;
        let manifest = CloudManifest::default();
        manifest.validate()?;
        let profile_path = device_profile_path(&device_profile.device.id);
        let descriptor = CloudNamespaceDescriptor::default();

        self.write_verified(SHARED_LIBRARY_PATH, &pretty_bytes(shared_library)?)
            .await?;
        self.write_verified(CLOUD_MANIFEST_PATH, &pretty_bytes(&manifest)?)
            .await?;
        self.write_verified(
            DELETION_REGISTRY_PATH,
            &pretty_bytes(&DeletionRegistry::default())?,
        )
        .await?;
        self.write_verified(&profile_path, &pretty_bytes(device_profile)?)
            .await?;
        self.write_verified(V2_NAMESPACE_DESCRIPTOR_PATH, &pretty_bytes(&descriptor)?)
            .await?;

        match self.inspect().await? {
            CloudNamespaceClassification::SupportedV2 {
                shared_library: stored,
                manifest: stored_manifest,
                ..
            } if stored == *shared_library && stored_manifest == manifest => Ok(()),
            _ => Err(CloudLibraryBootstrapError::FinalVerificationMismatch),
        }
    }

    async fn write_verified(
        &self,
        path: &str,
        bytes: &[u8],
    ) -> Result<(), CloudLibraryBootstrapError> {
        for _ in 0..self.max_attempts {
            self.transport.write(path, bytes).await?;
            if self.transport.read(path).await?.as_deref() == Some(bytes) {
                return Ok(());
            }
        }
        Err(CloudLibraryBootstrapError::WriteVerificationFailed {
            path: path.to_string(),
            attempts: self.max_attempts,
        })
    }
}

fn pretty_bytes(value: &impl Serialize) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec_pretty(value)
}

#[derive(Debug, Error)]
pub enum CloudLibraryBootstrapError {
    #[error(transparent)]
    Namespace(#[from] CloudNamespaceError),
    #[error(transparent)]
    Ownership(#[from] OwnershipError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("Cloud Library serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Cloud Library transport error: {0}")]
    Transport(#[from] opendal::Error),
    #[error("Cloud root changed before creation: {0}")]
    RootChanged(&'static str),
    #[error("Cloud object {path} did not match after {attempts} write attempts")]
    WriteVerificationFailed { path: String, attempts: usize },
    #[error("Created Cloud Library did not match the intended state")]
    FinalVerificationMismatch,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use super::*;
    use crate::config::V2_CONFIG_SCHEMA_VERSION;

    #[derive(Default)]
    struct State {
        objects: BTreeMap<String, Vec<u8>>,
        writes: Vec<String>,
        corrupt_writes: bool,
    }

    #[derive(Default)]
    struct FakeTransport {
        state: Mutex<State>,
    }

    #[async_trait]
    impl NamespaceTransport for FakeTransport {
        async fn read(&self, path: &str) -> Result<Option<Vec<u8>>, opendal::Error> {
            Ok(self.state.lock().unwrap().objects.get(path).cloned())
        }

        async fn list_sample(
            &self,
            prefix: &str,
            limit: usize,
        ) -> Result<Vec<String>, opendal::Error> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .objects
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
            let mut state = self.state.lock().unwrap();
            state.writes.push(path.to_string());
            let stored = if state.corrupt_writes {
                b"corrupt".to_vec()
            } else {
                bytes.to_vec()
            };
            state.objects.insert(path.to_string(), stored);
            Ok(())
        }
    }

    fn inputs() -> (SharedLibrary, DeviceProfile) {
        let owners = crate::config::ConfigurationOwners::from_legacy(
            &crate::config::Config::default(),
            &"device".to_string(),
        );
        (
            SharedLibrary {
                schema_version: V2_CONFIG_SCHEMA_VERSION,
                games: Vec::new(),
            },
            owners.device_profiles["device"].clone(),
        )
    }

    #[tokio::test]
    async fn creates_required_objects_with_descriptor_last() {
        let transport = FakeTransport::default();
        let bootstrap = CloudLibraryBootstrap::with_transport(transport, 2);
        let (shared, profile) = inputs();

        bootstrap.create_empty(&shared, &profile).await.unwrap();

        let state = bootstrap.transport.state.lock().unwrap();
        assert_eq!(
            state.writes.last().map(String::as_str),
            Some(V2_NAMESPACE_DESCRIPTOR_PATH)
        );
        assert!(state.objects.contains_key(&device_profile_path("device")));
        assert!(state.objects.contains_key(DELETION_REGISTRY_PATH));
    }

    #[tokio::test]
    async fn refuses_non_empty_and_partial_roots_without_writing() {
        let transport = FakeTransport::default();
        transport
            .state
            .lock()
            .unwrap()
            .objects
            .insert("unrelated".into(), Vec::new());
        let bootstrap = CloudLibraryBootstrap::with_transport(transport, 2);
        let (shared, profile) = inputs();

        assert!(matches!(
            bootstrap.create_empty(&shared, &profile).await,
            Err(CloudLibraryBootstrapError::Namespace(
                CloudNamespaceError::UnrecognizedRoot(_)
            ))
        ));
        assert!(bootstrap.transport.state.lock().unwrap().writes.is_empty());
    }

    #[tokio::test]
    async fn read_back_mismatch_never_publishes_descriptor() {
        let transport = FakeTransport::default();
        transport.state.lock().unwrap().corrupt_writes = true;
        let bootstrap = CloudLibraryBootstrap::with_transport(transport, 2);
        let (shared, profile) = inputs();

        assert!(matches!(
            bootstrap.create_empty(&shared, &profile).await,
            Err(CloudLibraryBootstrapError::WriteVerificationFailed { attempts: 2, .. })
        ));
        let state = bootstrap.transport.state.lock().unwrap();
        assert!(!state.objects.contains_key(V2_NAMESPACE_DESCRIPTOR_PATH));
    }
}
