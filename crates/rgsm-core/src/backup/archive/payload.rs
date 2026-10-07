use crate::preclude::{BackupFileError, CompressError};

/// Decode historical registry JSON and ordinary .reg entries at the archive
/// boundary, before either migration or restoration consumes registry data.
pub(super) fn read_registry_payload(
    name: &str,
    bytes: &[u8],
) -> Result<crate::backup::registry::RegistryData, CompressError> {
    if name.ends_with(".json") {
        serde_json::from_slice(bytes).map_err(|error| {
            CompressError::Single(BackupFileError::RegistryError(error.to_string()))
        })
    } else {
        crate::backup::registry::deserialize_reg_file(bytes).map_err(|error| {
            CompressError::Single(BackupFileError::RegistryError(error.to_string()))
        })
    }
}
