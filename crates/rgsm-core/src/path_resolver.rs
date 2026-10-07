use std::path::PathBuf;
use thiserror::Error;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::config::Config;

/// Errors that may occur during path resolution
#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("{0}")]
    Selection(String),

    #[error("Path {0} requires a resolution context")]
    MissingContext(String),
}

/// Result of checking a single path
#[derive(Debug, Serialize, Deserialize, Clone, Type, utoipa::ToSchema)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum PathCheckResult {
    /// Path resolved and exists on filesystem
    #[serde(rename_all = "camelCase")]
    Ok {
        raw_path: String,
        resolved_path: String,
        is_file: bool,
    },
    /// Path resolved but doesn't exist on filesystem
    #[serde(rename_all = "camelCase")]
    NotFound {
        raw_path: String,
        resolved_path: String,
    },
    /// Registry path
    #[serde(rename_all = "camelCase")]
    RegistryPath {
        raw_path: String,
        /// Whether the registry key exists (always `false` on non-Windows).
        exists: bool,
        /// Whether registry operations are supported on this platform.
        supported: bool,
        /// Optional portable spelling for this process's own user, never another SID.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        current_user_path: Option<String>,
    },
    /// Failed to resolve path variables
    #[serde(rename_all = "camelCase")]
    ResolveFailed { raw_path: String, error: String },
}

/// Check a single path: resolve variables and check filesystem status
pub fn check_path(raw_path: &str, ctx: Option<&PathContext>, _config: &Config) -> PathCheckResult {
    // Handle registry paths
    if crate::backup::registry::is_registry_path(raw_path) {
        #[cfg(target_os = "windows")]
        {
            let exists = match crate::backup::registry::registry_key_exists(raw_path) {
                Ok(exists) => exists,
                Err(error) => {
                    return PathCheckResult::ResolveFailed {
                        raw_path: raw_path.to_string(),
                        error: error.to_string(),
                    };
                }
            };
            return PathCheckResult::RegistryPath {
                raw_path: raw_path.to_string(),
                exists,
                supported: true,
                current_user_path: crate::backup::registry::suggest_current_user_path(raw_path),
            };
        }
        #[cfg(not(target_os = "windows"))]
        {
            return PathCheckResult::RegistryPath {
                raw_path: raw_path.to_string(),
                exists: false,
                supported: false,
                current_user_path: None,
            };
        }
    }

    // Try to resolve the path
    match resolve_path_explicit(raw_path, ctx) {
        Ok(resolved) => {
            let resolved_str = resolved.to_string_lossy().to_string();
            if resolved.exists() {
                PathCheckResult::Ok {
                    raw_path: raw_path.to_string(),
                    resolved_path: resolved_str,
                    is_file: resolved.is_file(),
                }
            } else {
                PathCheckResult::NotFound {
                    raw_path: raw_path.to_string(),
                    resolved_path: resolved_str,
                }
            }
        }
        Err(e) => PathCheckResult::ResolveFailed {
            raw_path: raw_path.to_string(),
            error: e.to_string(),
        },
    }
}

/// Check multiple paths at once
pub fn check_paths(
    paths: &[String],
    ctx: Option<&PathContext>,
    config: &Config,
) -> Vec<PathCheckResult> {
    paths.iter().map(|p| check_path(p, ctx, config)).collect()
}

/// Exact paths and patterns share the same immutable resolution context.
pub type PathContext = crate::path_resolution::ResolutionContext;

pub fn resolve_path_explicit(
    raw_path: &str,
    context: Option<&PathContext>,
) -> Result<PathBuf, ResolveError> {
    if !raw_path.contains('<') && !raw_path.contains('>') {
        return Ok(raw_path.into());
    }
    let context = context.ok_or_else(|| ResolveError::MissingContext(raw_path.to_string()))?;
    crate::path_resolution::resolve_single_path(raw_path, context).map_err(ResolveError::Selection)
}

#[cfg(test)]
mod tests;
