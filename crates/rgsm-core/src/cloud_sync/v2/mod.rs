mod bootstrap;
mod conflict_resolution;
mod conflict_review;
mod cutover;
mod deletion;
mod deletion_registry;
mod game_comparison;
pub(crate) mod game_deletion;
mod integrity;
mod join;
mod local_eviction;
mod manifest;
mod materialization;
mod materialization_models;
#[cfg(test)]
mod materialization_tests;
mod namespace;
mod profile_removal;
mod profile_repository;
mod remote_resolution;
mod repository;
mod retention;
mod shared_library_repository;
mod snapshot_sync;

pub use bootstrap::{CloudLibraryBootstrap, CloudLibraryBootstrapError, CloudLibraryTransport};
pub use conflict_resolution::{
    KeepLocalProgressError, KeepLocalProgressOutcome, V2ConflictResolver,
};
pub use conflict_review::{
    ConflictReviewError, LocalProgressView, ProgressRelation, RemoteProgressCandidate,
    V2ConflictInspector, V2ConflictReview, progress_requires_choice,
};
pub use cutover::{
    CloudLibraryCutover, CloudLibraryCutoverError, CloudLibraryCutoverResult,
    CloudLibraryCutoverReview,
};
pub use deletion::{
    ArchiveDeletionBackend, ArchiveDeletionError, GlobalSnapshotDeletion,
    GlobalSnapshotDeletionError, OpenDalArchiveDeletionBackend, SnapshotDeletionLifecycle,
    SnapshotDeletionLifecycleError, SnapshotDeletionRequest,
};
pub use deletion_registry::{
    DELETION_REGISTRY_SCHEMA_VERSION, DeletionRegistry, DeletionRegistryError,
    DeletionRegistryRepository, GameDeletion, ProfileDeletion,
};
pub use game_comparison::{
    GameJoinCandidate, GameJoinClassification, GameJoinComparisonError, compare_join_libraries,
};
pub use game_deletion::{SharedGameDeletion, SharedGameDeletionError, SharedGameDeletionOutcome};
pub use integrity::{ArchiveIntegrity, ArchiveIntegrityError};
pub use join::{
    CloudLibraryJoin, CloudLibraryJoinError, CloudLibraryJoinItem, CloudLibraryJoinReview,
    GameDefinitionDifference, JoinGameAction, JoinGameDecision,
};
pub use local_eviction::{
    CloudArchiveEviction, CloudArchiveEvictionError, LocalArchiveEviction,
    LocalArchiveEvictionError,
};
pub use manifest::{
    CLOUD_MANIFEST_SCHEMA_VERSION, CloudManifest, DeletionKind, GameManifest, LiveSnapshot,
    ManifestError, PendingTombstone, SnapshotNode, SnapshotState,
};
pub use materialization::{
    CloudArchiveGameView, CloudArchiveLibraryView, CloudArchiveMaterializer,
    CloudArchiveSnapshotView, MaterializationError,
};
pub use materialization_models::{
    CloudArchiveDeletionView, MaterializationOutcome, MaterializationPreview,
};
pub use namespace::{
    CLOUD_ARCHIVES_PREFIX, CLOUD_MANIFEST_PATH, CloudNamespaceClassification,
    CloudNamespaceClassifier, CloudNamespaceDescriptor, CloudNamespaceError,
    DELETION_REGISTRY_PATH, NamespaceTransport, OpenDalNamespaceTransport, SHARED_LIBRARY_PATH,
    V2_DEVICE_PROFILES_PREFIX, V2_NAMESPACE_DESCRIPTOR_PATH, V2_NAMESPACE_PREFIX,
    V2_NAMESPACE_SCHEMA_VERSION, cloud_archive_path, device_profile_path,
};
pub use profile_removal::{
    DeviceProfileRemoval, DeviceProfileRemovalError, DeviceProfileRemovalOutcome,
};
pub use profile_repository::{DeviceProfileRepository, DeviceProfileRepositoryError};
pub use remote_resolution::{
    AcceptRemoteProgressError, PreparedRemoteProgress, V2RemoteProgressResolver,
};
pub use repository::{
    CloudManifestRepository, ManifestRepositoryError, ManifestTransport, OpenDalManifestTransport,
};
pub use retention::{
    SnapshotRetentionPlan, SnapshotRetentionPlanner, SnapshotRetentionPlannerError,
};
pub use shared_library_repository::{SharedLibraryRepository, SharedLibraryRepositoryError};
pub use snapshot_sync::{
    RetentionEnforcementOutcome, SnapshotReconcilePolicy, SnapshotReconciliationOutcome,
    SnapshotSyncCoordinator, SnapshotSyncError,
};
