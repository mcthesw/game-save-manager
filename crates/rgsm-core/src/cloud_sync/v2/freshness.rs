use opendal::{Entry, Operator};

use super::{
    CLOUD_MANIFEST_PATH, DELETION_REGISTRY_PATH, SHARED_LIBRARY_PATH, V2_DEVICE_PROFILES_PREFIX,
    V2_NAMESPACE_DESCRIPTOR_PATH, V2_NAMESPACE_PREFIX,
};

const REQUIRED_OBJECTS: [&str; 4] = [
    V2_NAMESPACE_DESCRIPTOR_PATH,
    SHARED_LIBRARY_PATH,
    CLOUD_MANIFEST_PATH,
    DELETION_REGISTRY_PATH,
];

/// A cheap fingerprint of everything a reconciliation round reads, taken with
/// one directory listing instead of one read per object.
///
/// Returns `None` when the provider's listing cannot vouch for a change (no
/// modification time or entity tag, or an object missing): the caller must then
/// treat the library as changed.
pub(crate) async fn library_signature(
    operator: &Operator,
) -> Result<Option<String>, opendal::Error> {
    let entries = operator.list(V2_NAMESPACE_PREFIX).await?;
    let mut parts = Vec::with_capacity(REQUIRED_OBJECTS.len() + 1);
    for path in REQUIRED_OBJECTS {
        let Some(entry) = entries.iter().find(|entry| entry.path() == path) else {
            return Ok(None);
        };
        let Some(part) = describe(operator, entry).await? else {
            return Ok(None);
        };
        parts.push(part);
    }
    // Other devices' profiles live in a directory some providers do not stamp.
    if let Some(entry) = entries
        .iter()
        .find(|entry| entry.path() == V2_DEVICE_PROFILES_PREFIX)
        && let Some(part) = describe(operator, entry).await?
    {
        parts.push(part);
    }
    Ok(Some(parts.join(";")))
}

async fn describe(operator: &Operator, entry: &Entry) -> Result<Option<String>, opendal::Error> {
    let mut metadata = entry.metadata().clone();
    // A local directory lists names only; stat is free there, never over the network.
    if metadata.last_modified().is_none()
        && metadata.etag().is_none()
        && operator.info().scheme() == "fs"
    {
        metadata = operator.stat(entry.path()).await?;
    }
    if metadata.last_modified().is_none() && metadata.etag().is_none() {
        return Ok(None);
    }
    Ok(Some(format!(
        "{}|{}|{}|{}",
        entry.path(),
        metadata
            .last_modified()
            .map(|time| time.to_string())
            .unwrap_or_default(),
        metadata.etag().unwrap_or_default(),
        metadata.content_length(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendal::services::Memory;

    fn operator() -> Operator {
        Operator::new(Memory::default()).unwrap().finish()
    }

    async fn write_library(operator: &Operator) {
        for path in REQUIRED_OBJECTS {
            operator.write(path, b"{}".to_vec()).await.unwrap();
        }
    }

    #[tokio::test]
    async fn a_listing_without_modification_evidence_is_not_trusted() {
        let operator = operator();
        write_library(&operator).await;
        assert_eq!(library_signature(&operator).await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_missing_object_is_not_trusted() {
        let root = temp_dir::TempDir::new().unwrap();
        let operator = Operator::new(
            opendal::services::Fs::default().root(root.path().to_string_lossy().as_ref()),
        )
        .unwrap()
        .finish();
        write_library(&operator).await;
        operator.delete(CLOUD_MANIFEST_PATH).await.unwrap();
        assert_eq!(library_signature(&operator).await.unwrap(), None);
    }

    #[tokio::test]
    async fn the_signature_changes_when_an_object_is_rewritten() {
        let root = temp_dir::TempDir::new().unwrap();
        let operator = Operator::new(
            opendal::services::Fs::default().root(root.path().to_string_lossy().as_ref()),
        )
        .unwrap()
        .finish();
        write_library(&operator).await;
        let first = library_signature(&operator).await.unwrap().unwrap();
        assert_eq!(library_signature(&operator).await.unwrap().unwrap(), first);

        operator
            .write(CLOUD_MANIFEST_PATH, b"{\"revision\":2}".to_vec())
            .await
            .unwrap();
        assert_ne!(library_signature(&operator).await.unwrap().unwrap(), first);
    }
}
