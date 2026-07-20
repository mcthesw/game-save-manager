mod bootstrap;
mod deletion;
mod game_comparison;
mod integrity;
mod manifest;
mod namespace;
mod repository;

pub use bootstrap::{CloudLibraryBootstrap, CloudLibraryBootstrapError, CloudLibraryTransport};
pub use deletion::{
    ArchiveDeletionBackend, ArchiveDeletionError, GlobalSnapshotDeletion,
    GlobalSnapshotDeletionError, OpenDalArchiveDeletionBackend, SnapshotDeletionRequest,
};
pub use game_comparison::{
    GameJoinCandidate, GameJoinClassification, GameJoinComparisonError, compare_join_libraries,
};
pub use integrity::{ArchiveIntegrity, ArchiveIntegrityError};
pub use manifest::{
    CLOUD_MANIFEST_SCHEMA_VERSION, CloudManifest, DeletionKind, GameManifest, LiveSnapshot,
    ManifestError, PendingTombstone, SnapshotNode, SnapshotState,
};
pub use namespace::{
    CLOUD_ARCHIVES_PREFIX, CLOUD_MANIFEST_PATH, CloudNamespaceClassification,
    CloudNamespaceClassifier, CloudNamespaceDescriptor, CloudNamespaceError, NamespaceTransport,
    OpenDalNamespaceTransport, SHARED_LIBRARY_PATH, V2_DEVICE_PROFILES_PREFIX,
    V2_NAMESPACE_DESCRIPTOR_PATH, V2_NAMESPACE_PREFIX, V2_NAMESPACE_SCHEMA_VERSION,
    device_profile_path,
};
pub use repository::{
    CloudManifestRepository, ManifestRepositoryError, ManifestTransport, OpenDalManifestTransport,
};
