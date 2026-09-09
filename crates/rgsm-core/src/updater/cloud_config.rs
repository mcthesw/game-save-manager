use semver::Version;

use super::{migration::migrate_config_schema, versions::VERSION_1_9_0};
use crate::{config::Config, preclude::UpdaterError};

/// Decode a V1 cloud configuration without touching this machine's configuration
/// or discovering its resources. Before 1.9, cloud directories used game names.
pub(crate) fn decode_legacy_cloud_config(content: &str) -> Result<Config, UpdaterError> {
    let original: Config = serde_json::from_str(content)?;
    let version = Version::parse(&original.version)?;
    if version >= Version::parse(VERSION_1_9_0)? {
        return Ok(original);
    }
    let mut config = migrate_config_schema(content, &version, None, &[])?;
    for game in &mut config.games {
        if game.storage_key.is_empty() {
            game.storage_key = game.name.clone();
        }
    }
    config.bind_legacy_game_references();
    Ok(config)
}
