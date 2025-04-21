use std::env;
use std::path::PathBuf;
use thiserror::Error;

use crate::backup::Game;
use crate::config::Config;

/// Errors that may occur during path resolution
#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("Unknown variable: {0}")]
    UnknownVariable(String),

    #[error("Cannot get environment variable: {0}")]
    DirNotFound(String),

    #[error("Unimplemented variable: {0}")]
    UnimplementedVar(String),

    #[error("Path conversion error: {0}")]
    PathConversion(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Resolves a path string containing variables to an actual filesystem path
///
/// # Arguments
///
/// * `raw_path` - The original path string containing variables
/// * `game` - Optional game information, used to resolve <game> variable
/// * `config` - Global configuration, used to resolve <root> variable
///
/// # Returns
///
/// The resolved absolute path on success, or an error on failure
pub fn resolve_path(
    raw_path: &str,
    _game: Option<&Game>,
    _config: &Config,
) -> Result<PathBuf, ResolveError> {
    // If the path doesn't contain variables, return it directly
    if !raw_path.contains('<') && !raw_path.contains('>') {
        return Ok(PathBuf::from(raw_path));
    }

    let mut result = raw_path.to_string();

    // Resolve <home> variable
    if result.contains("<home>") {
        let home_dir =
            dirs::home_dir().ok_or(ResolveError::DirNotFound("Home directory".to_string()))?;
        let home_str = home_dir.to_str().ok_or_else(|| {
            ResolveError::PathConversion("Cannot convert home directory path to string".to_string())
        })?;
        result = result.replace("<home>", home_str);
    }

    // Resolve <osUserName> variable
    if result.contains("<osUserName>") {
        let username = whoami::username();
        result = result.replace("<osUserName>", &username);
    }

    // Resolve <root> variable
    if result.contains("<root>") {
        return Err(ResolveError::UnimplementedVar("<root>".to_string()));
    }

    // Resolve <game> variable
    if result.contains("<game>") {
        return Err(ResolveError::UnimplementedVar("<game>".to_string()));
    }

    // Resolve <base> variable (depends on <root> and <game>)
    if result.contains("<base>") {
        return Err(ResolveError::UnimplementedVar("<base>".to_string()));
    }

    // Windows specific variables
    // Resolve <winAppData> variable
    if result.contains("<winAppData>") {
        let app_data = dirs::data_dir()
            .ok_or(ResolveError::DirNotFound("APPDATA".to_string()))?
            .to_str()
            .ok_or_else(|| {
                ResolveError::PathConversion("Cannot convert AppData path to string".to_string())
            })?
            .to_string();
        result = result.replace("<winAppData>", &app_data);
    }

    // Resolve <winLocalAppData> variable
    if result.contains("<winLocalAppData>") {
        let local_app_data = dirs::data_local_dir()
            .ok_or(ResolveError::DirNotFound("LOCALAPPDATA".to_string()))?
            .to_str()
            .ok_or_else(|| {
                ResolveError::PathConversion(
                    "Cannot convert LocalAppData path to string".to_string(),
                )
            })?
            .to_string();
        result = result.replace("<winLocalAppData>", &local_app_data);
    }

    // Resolve <winLocalAppDataLow> variable
    if result.contains("<winLocalAppDataLow>") {
        let home_dir =
            dirs::home_dir().ok_or(ResolveError::DirNotFound("Home directory".to_string()))?;
        let local_app_data_low = home_dir.join("AppData").join("LocalLow");
        let local_app_data_low_str = local_app_data_low.to_str().ok_or_else(|| {
            ResolveError::PathConversion(
                "Cannot convert LocalAppDataLow path to string".to_string(),
            )
        })?;
        result = result.replace("<winLocalAppDataLow>", local_app_data_low_str);
    }

    // Resolve <winDocuments> variable
    if result.contains("<winDocuments>") {
        let documents = dirs::document_dir()
            .ok_or(ResolveError::DirNotFound("Documents".to_string()))?
            .to_str()
            .ok_or_else(|| {
                ResolveError::PathConversion("Cannot convert Documents path to string".to_string())
            })?
            .to_string();
        result = result.replace("<winDocuments>", &documents);
    }

    // Resolve <winPublic> variable
    if result.contains("<winPublic>") {
        let public =
            env::var("PUBLIC").map_err(|_| ResolveError::DirNotFound("PUBLIC".to_string()))?;
        result = result.replace("<winPublic>", &public);
    }

    // Resolve <winProgramData> variable
    if result.contains("<winProgramData>") {
        let program_data = env::var("PROGRAMDATA")
            .map_err(|_| ResolveError::DirNotFound("PROGRAMDATA".to_string()))?;
        result = result.replace("<winProgramData>", &program_data);
    }

    // Resolve <winDir> variable
    if result.contains("<winDir>") {
        let win_dir =
            env::var("WINDIR").map_err(|_| ResolveError::DirNotFound("WINDIR".to_string()))?;
        result = result.replace("<winDir>", &win_dir);
    }

    // Linux specific variables

    // Resolve <xdgData> variable
    if result.contains("<xdgData>") {
        let xdg_data = dirs::data_dir()
            .ok_or(ResolveError::DirNotFound("XDG_DATA_HOME".to_string()))?
            .to_str()
            .ok_or_else(|| {
                ResolveError::PathConversion(
                    "Cannot convert XDG_DATA_HOME path to string".to_string(),
                )
            })?
            .to_string();
        result = result.replace("<xdgData>", &xdg_data);
    }

    // Resolve <xdgConfig> variable
    if result.contains("<xdgConfig>") {
        let xdg_config = dirs::config_dir()
            .ok_or(ResolveError::DirNotFound("XDG_CONFIG_HOME".to_string()))?
            .to_str()
            .ok_or_else(|| {
                ResolveError::PathConversion(
                    "Cannot convert XDG_CONFIG_HOME path to string".to_string(),
                )
            })?
            .to_string();
        result = result.replace("<xdgConfig>", &xdg_config);
    }

    // Check for unresolved variables
    if result.contains('<') && result.contains('>') {
        // Extract the unresolved variable name
        let start = result.find('<').unwrap();
        let end = result[start..]
            .find('>')
            .map(|pos| start + pos + 1)
            .unwrap_or(result.len());
        let var_name = &result[start..end];

        return Err(ResolveError::UnknownVariable(var_name.to_string()));
    }

    Ok(PathBuf::from(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    // Create test configuration
    fn create_test_config() -> Config {
        Config {
            version: "1.0.0".to_string(),
            backup_path: "/test/backup".to_string(),
            games: Vec::new(),
            settings: crate::config::Settings::default(),
            favorites: Vec::new(),
            quick_action: crate::config::QuickActionsSettings::default(),
            devices: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_resolve_path_without_variables() {
        let config = create_test_config();
        let path = "/simple/path/without/variables";

        let result = resolve_path(path, None, &config).unwrap();
        assert_eq!(result, PathBuf::from(path));
    }

    #[test]
    fn test_resolve_home_variable() {
        let config = create_test_config();
        let path = "<home>/Documents/saves";

        let result = resolve_path(path, None, &config);
        assert!(result.is_ok()); // Actual value depends on the runtime environment
    }

    #[test]
    fn test_resolve_os_username_variable() {
        let config = create_test_config();
        let path = "/Users/<osUserName>/Documents";

        let result = resolve_path(path, None, &config);
        assert!(result.is_ok()); // Actual value depends on the runtime environment
    }

    #[test]
    fn test_error_on_unknown_variable() {
        let config = create_test_config();
        let path = "<unknown>/saves";

        let result = resolve_path(path, None, &config);
        assert!(matches!(result, Err(ResolveError::UnknownVariable(_))));
    }

    // Linux specific tests
    #[cfg(target_os = "linux")]
    mod linux_tests {
        use super::*;

        #[test]
        fn test_resolve_linux_variables() {
            let config = create_test_config();

            // Test Linux specific variables
            let paths = vec!["<xdgData>/saves", "<xdgConfig>/saves"];

            for path in paths {
                let result = resolve_path(path, None, &config);
                assert!(result.is_ok(), "Failed to resolve path: {}", path);
            }
        }
    }
}
