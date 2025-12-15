use std::env;
use std::path::PathBuf;
use thiserror::Error;

use serde::{Deserialize, Serialize};
use specta::Type;

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

/// Result of checking a single path
#[derive(Debug, Serialize, Deserialize, Clone, Type)]
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
    /// Registry path (not supported for backup)
    #[serde(rename_all = "camelCase")]
    RegistryNotSupported { raw_path: String },
    /// Failed to resolve path variables
    #[serde(rename_all = "camelCase")]
    ResolveFailed { raw_path: String, error: String },
}

/// Check a single path: resolve variables and check filesystem status
pub fn check_path(raw_path: &str, config: &Config) -> PathCheckResult {
    // Handle registry paths
    if raw_path.starts_with("REGISTRY:") || raw_path.starts_with("HKEY_") {
        return PathCheckResult::RegistryNotSupported {
            raw_path: raw_path.to_string(),
        };
    }

    // Try to resolve the path
    match resolve_path(raw_path, None, config) {
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
pub fn check_paths(paths: &[String], config: &Config) -> Vec<PathCheckResult> {
    paths.iter().map(|p| check_path(p, config)).collect()
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

    // Resolve <root> variable (Steam install directory)
    // This is typically used in ludusavi manifest for Steam game paths
    // Format: <root>/userdata/<storeuserid>/...
    if result.contains("<root>") {
        let steam_root = get_steam_root()?;
        result = result.replace("<root>", &steam_root);
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

    // Resolve <storeuserid> variable (Steam user ID)
    if result.contains("<storeuserid>") {
        let steam_root = get_steam_root()?;
        let userdata_path = std::path::Path::new(&steam_root).join("userdata");

        let mut candidates: Vec<(std::time::SystemTime, String)> = Vec::new();
        for entry in std::fs::read_dir(&userdata_path)
            .map_err(|_| ResolveError::DirNotFound("Steam userdata directory".to_string()))?
            .flatten()
        {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let Some(dir_name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            if !dir_name.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }

            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            candidates.push((modified, dir_name));
        }

        candidates.sort_by_key(|(modified, _)| *modified);
        let Some((_, user_id)) = candidates.pop() else {
            return Err(ResolveError::DirNotFound("Steam user id".to_string()));
        };

        result = result.replace("<storeuserid>", &user_id);
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

/// Try to get Steam installation path from Windows Registry
#[cfg(target_os = "windows")]
fn get_steam_path_from_registry() -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::*;

    // Try HKEY_CURRENT_USER first (user-specific installation)
    if let Ok(hkcu) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam") {
        if let Ok(path) = hkcu.get_value::<String, _>("SteamPath") {
            let normalized = path.replace('/', "\\");
            if std::path::Path::new(&normalized).exists() {
                return Some(normalized);
            }
        }
    }

    // Try HKEY_LOCAL_MACHINE (machine-wide installation, 32-bit on 64-bit Windows)
    if let Ok(hklm) =
        RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("SOFTWARE\\WOW6432Node\\Valve\\Steam")
    {
        if let Ok(path) = hklm.get_value::<String, _>("InstallPath") {
            let normalized = path.replace('/', "\\");
            if std::path::Path::new(&normalized).exists() {
                return Some(normalized);
            }
        }
    }

    // Try HKEY_LOCAL_MACHINE (32-bit Windows or native key)
    if let Ok(hklm) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("SOFTWARE\\Valve\\Steam") {
        if let Ok(path) = hklm.get_value::<String, _>("InstallPath") {
            let normalized = path.replace('/', "\\");
            if std::path::Path::new(&normalized).exists() {
                return Some(normalized);
            }
        }
    }

    None
}

/// Get potential Steam paths from all available drives on Windows
#[cfg(target_os = "windows")]
fn get_steam_paths_from_all_drives() -> Vec<String> {
    let mut paths = Vec::new();

    // Check drives A-Z
    for letter in b'A'..=b'Z' {
        let drive = format!("{}:", letter as char);
        let drive_path = std::path::Path::new(&drive);

        // Skip if drive doesn't exist
        if !drive_path.exists() {
            continue;
        }

        // Common Steam installation paths on each drive
        let potential_paths = [
            format!("{}\\Program Files (x86)\\Steam", drive),
            format!("{}\\Program Files\\Steam", drive),
            format!("{}\\Steam", drive),
            format!("{}\\SteamLibrary", drive),
        ];

        for path in potential_paths {
            // Only add paths that we haven't checked yet via other methods
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }

    paths
}

/// Helper function to get Steam root directory
fn get_steam_root() -> Result<String, ResolveError> {
    let mut steam_roots: Vec<String> = Vec::new();

    // First, try environment variable
    if let Ok(env_root) = env::var("STEAM_DIR") {
        if !env_root.trim().is_empty() {
            steam_roots.push(env_root);
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Try reading from Windows Registry first (most reliable)
        if let Some(reg_path) = get_steam_path_from_registry() {
            steam_roots.insert(0, reg_path);
        }

        // Fallback to common paths with PROGRAMFILES environment variables
        if let Ok(pf86) = env::var("PROGRAMFILES(X86)") {
            steam_roots.push(format!("{}\\Steam", pf86.trim_end_matches('\\')));
        }
        if let Ok(pf) = env::var("PROGRAMFILES") {
            steam_roots.push(format!("{}\\Steam", pf.trim_end_matches('\\')));
        }

        // Check all available drives for Steam installation
        steam_roots.extend(get_steam_paths_from_all_drives());
    }

    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().unwrap_or_default();
        steam_roots.push(home.join(".steam/steam").to_string_lossy().to_string());
        steam_roots.push(
            home.join(".local/share/Steam")
                .to_string_lossy()
                .to_string(),
        );
        steam_roots.push(
            home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam")
                .to_string_lossy()
                .to_string(),
        );
    }

    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().unwrap_or_default();
        steam_roots.push(
            home.join("Library/Application Support/Steam")
                .to_string_lossy()
                .to_string(),
        );
    }

    steam_roots
        .into_iter()
        .find(|path| std::path::Path::new(path).exists())
        .ok_or(ResolveError::DirNotFound(
            "Steam root directory".to_string(),
        ))
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
