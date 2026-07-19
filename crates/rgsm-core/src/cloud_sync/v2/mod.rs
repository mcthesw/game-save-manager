mod bootstrap;
mod cutover;
mod deletion;
mod game_comparison;
mod integrity;
mod join;
mod manifest;
mod materialization;
#[cfg(test)]
mod materialization_tests;
mod namespace;
mod profile_repository;
mod repository;
mod snapshot_sync;

pub use bootstrap::{CloudLibraryBootstrap, CloudLibraryBootstrapError, CloudLibraryTransport};
pub use cutover::{
    CloudLibraryCutover, CloudLibraryCutoverError, CloudLibraryCutoverResult,
    CloudLibraryCutoverReview,
};
pub use deletion::{
    ArchiveDeletionBackend, ArchiveDeletionError, GlobalSnapshotDeletion,
    GlobalSnapshotDeletionError, OpenDalArchiveDeletionBackend, SnapshotDeletionRequest,
};
pub use game_comparison::{
    GameJoinCandidate, GameJoinClassification, GameJoinComparisonError, compare_join_libraries,
};
pub use integrity::{ArchiveIntegrity, ArchiveIntegrityError};
pub use join::{
    CloudLibraryJoin, CloudLibraryJoinError, CloudLibraryJoinItem, CloudLibraryJoinReview,
    GameDefinitionDifference, JoinGameAction, JoinGameDecision,
};
pub use manifest::{
    CLOUD_MANIFEST_SCHEMA_VERSION, CloudManifest, DeletionKind, GameManifest, LiveSnapshot,
    ManifestError, PendingTombstone, SnapshotNode, SnapshotState,
};
pub use materialization::{
    CloudArchiveGameView, CloudArchiveLibraryView, CloudArchiveMaterializer,
    CloudArchiveSnapshotView, MaterializationError, MaterializationOutcome, MaterializationPreview,
};
pub use namespace::{
    CLOUD_ARCHIVES_PREFIX, CLOUD_MANIFEST_PATH, CloudNamespaceClassification,
    CloudNamespaceClassifier, CloudNamespaceDescriptor, CloudNamespaceError, NamespaceTransport,
    OpenDalNamespaceTransport, SHARED_LIBRARY_PATH, V2_DEVICE_PROFILES_PREFIX,
    V2_NAMESPACE_DESCRIPTOR_PATH, V2_NAMESPACE_PREFIX, V2_NAMESPACE_SCHEMA_VERSION,
    cloud_archive_path, device_profile_path,
};
pub use profile_repository::{DeviceProfileRepository, DeviceProfileRepositoryError};
pub use repository::{
    CloudManifestRepository, ManifestRepositoryError, ManifestTransport, OpenDalManifestTransport,
};
pub use snapshot_sync::{
    SnapshotReconciliationOutcome, SnapshotSyncCoordinator, SnapshotSyncError,
};
