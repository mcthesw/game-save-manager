use opendal::{Operator, services};

use super::{CLOUD_MANIFEST_PATH, CloudManifest, DELETION_REGISTRY_PATH, DeletionRegistry};

/// A bound library always carries its Manifest and deletion registry; readers
/// treat their absence as damage, so fixtures start from empty ones.
pub(super) async fn seed_library_metadata(operator: &Operator) {
    for (path, bytes) in [
        (
            CLOUD_MANIFEST_PATH,
            serde_json::to_vec_pretty(&CloudManifest::default()).unwrap(),
        ),
        (
            DELETION_REGISTRY_PATH,
            serde_json::to_vec_pretty(&DeletionRegistry::default()).unwrap(),
        ),
    ] {
        if !operator.exists(path).await.unwrap() {
            operator.write(path, bytes).await.unwrap();
        }
    }
}

pub(super) async fn seeded_memory_operator() -> Operator {
    let operator = Operator::new(services::Memory::default())
        .expect("memory backend should initialize")
        .finish();
    seed_library_metadata(&operator).await;
    operator
}
