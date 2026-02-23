use std::path::Path;

use crate::preclude::BackupFileError;

/// Convert a filesystem path into zip-style path (`/` separator).
pub(crate) fn path_to_zip_style(path: &Path) -> Result<String, BackupFileError> {
    let parts = path
        .iter()
        .map(|part| part.to_str().ok_or(BackupFileError::NonePathError))
        .collect::<Result<Vec<_>, BackupFileError>>()?;
    Ok(parts.join("/"))
}
