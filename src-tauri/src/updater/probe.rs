use std::fs;
use std::path::Path;

use log::error;
use semver::Version;
use serde_json::Value;

use crate::preclude::UpdaterError;

/// Probe the version field from a config file
///
/// This function only reads the "version" field from the JSON file without
/// deserializing the entire structure, which is more efficient for version checking.
///
/// # Arguments
/// * `path` - Path to the config file
///
/// # Returns
/// * `Ok(Version)` - The parsed version from the config file
/// * `Err(UpdaterError)` - If the version field is missing or invalid
pub fn probe_config_version<P: AsRef<Path>>(path: P) -> Result<Version, UpdaterError> {
    let content = fs::read_to_string(path.as_ref())?;
    let v: Value = serde_json::from_str(&content)?;

    if let Some(s) = v.get("version").and_then(Value::as_str) {
        Ok(Version::parse(s)?)
    } else {
        error!(target: "rgsm::updater", "Missing version field in config file");
        Err(UpdaterError::MissingVersion)
    }
}
