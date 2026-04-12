use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::config::Config;
use crate::steam::InstalledSteamGame;

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

    #[error("Game not installed on this device: {0}")]
    GameNotInstalled(String),

    #[error("Path variable {0} requires game context (PathContext) but none was provided")]
    MissingContext(String),

    #[error("Store not supported for <base> resolution: {0}")]
    StoreNotSupported(String),
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
    /// Registry path
    #[serde(rename_all = "camelCase")]
    RegistryPath {
        raw_path: String,
        /// Whether the registry key exists (always `false` on non-Windows).
        exists: bool,
        /// Whether registry operations are supported on this platform.
        supported: bool,
    },
    /// Failed to resolve path variables
    #[serde(rename_all = "camelCase")]
    ResolveFailed { raw_path: String, error: String },
}

/// Check a single path: resolve variables and check filesystem status
pub fn check_path(raw_path: &str, ctx: Option<&PathContext>, config: &Config) -> PathCheckResult {
    // Handle registry paths
    if raw_path.starts_with("REGISTRY:") || raw_path.starts_with("HKEY_") {
        #[cfg(target_os = "windows")]
        {
            let exists = crate::backup::registry::registry_key_exists(raw_path).unwrap_or(false);
            return PathCheckResult::RegistryPath {
                raw_path: raw_path.to_string(),
                exists,
                supported: true,
            };
        }
        #[cfg(not(target_os = "windows"))]
        {
            return PathCheckResult::RegistryPath {
                raw_path: raw_path.to_string(),
                exists: false,
                supported: false,
            };
        }
    }

    // Try to resolve the path
    match resolve_path(raw_path, ctx, config) {
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

/// Game-specific context for resolving Ludusavi path variables.
///
/// Built via `Game::path_context()` for per-game operations, or with
/// a shared `install_dir_cache` for bulk operations like `detect_local_games`.
#[derive(Debug, Clone, Default)]
pub struct PathContext {
    /// Install directory names from the manifest's `installDir` field.
    pub install_dirs: Vec<String>,
    /// Steam App ID from the manifest.
    pub steam_id: Option<u32>,
    /// Pre-computed map of lowercase(installDir) → `InstalledSteamGame` for bulk ops.
    pub install_dir_cache: Option<Arc<HashMap<String, InstalledSteamGame>>>,
    /// User-configured game root directories. First entry = `<root>`.
    pub game_roots: Vec<String>,
    /// Configured store user ID for `<storeUserId>`. Overrides auto-detection.
    pub store_user_id: Option<String>,
}

/// Resolves a path string containing Ludusavi variables (e.g. `<home>`, `<root>`) to a filesystem path.
pub fn resolve_path(
    raw_path: &str,
    ctx: Option<&PathContext>,
    _config: &Config,
) -> Result<PathBuf, ResolveError> {
    // If the path doesn't contain variables, return it directly
    if !raw_path.contains('<') && !raw_path.contains('>') {
        return Ok(PathBuf::from(raw_path));
    }

    let mut result = raw_path.to_string();

    if result.contains("<home>") {
        let home_dir =
            dirs::home_dir().ok_or(ResolveError::DirNotFound("Home directory".to_string()))?;
        let home_str = home_dir.to_str().ok_or_else(|| {
            ResolveError::PathConversion("Cannot convert home directory path to string".to_string())
        })?;
        result = result.replace("<home>", home_str);
    }

    // Resolve <osUserName>
    if result.contains("<osUserName>") {
        let username = whoami::username();
        result = result.replace("<osUserName>", &username);
    }

    // Resolve <root>: first configured game_root, else auto-detect Steam root
    if result.contains("<root>") {
        let root = if let Some(ctx) = ctx {
            ctx.game_roots
                .first()
                .cloned()
                .map_or_else(get_steam_root, Ok)?
        } else {
            get_steam_root()?
        };
        result = result.replace("<root>", &root);
    }

    // Resolve <game> and <base> variables (Ludusavi definitions):
    //   <game>  = installDir (if defined) or the game's canonical name
    //   <base>  = <root>/<game> (shorthand; store-specific rules may override)
    //   <storeGameId> = store-specific game ID from the manifest
    // <base> and <game> MUST come from the same installDir match for multi-alias games.
    let needs_game = result.contains("<game>");
    let needs_base = result.contains("<base>");
    let needs_store_game_id = result.contains("<storeGameId>");

    if needs_game || needs_base || needs_store_game_id {
        let ctx = ctx.ok_or_else(|| {
            let var = if needs_base {
                "<base>"
            } else if needs_game {
                "<game>"
            } else {
                "<storeGameId>"
            };
            ResolveError::MissingContext(var.to_string())
        })?;

        // Resolve <storeGameId> from steam_id
        if needs_store_game_id {
            let steam_id = ctx
                .steam_id
                .ok_or_else(|| ResolveError::MissingContext("<storeGameId>".to_string()))?;
            result = result.replace("<storeGameId>", &steam_id.to_string());
        }

        // Resolve <base> and/or <game> — both from the same installDir match
        if needs_base || needs_game {
            if ctx.install_dirs.is_empty() {
                return Err(ResolveError::MissingContext(
                    if needs_base { "<base>" } else { "<game>" }.to_string(),
                ));
            }

            let (matched_dir, install_path) = crate::steam::find_game_install_path(
                &ctx.install_dirs,
                ctx.install_dir_cache.as_deref(),
            )
            .ok_or_else(|| {
                ResolveError::GameNotInstalled(
                    ctx.install_dirs.first().cloned().unwrap_or_default(),
                )
            })?;

            if needs_base {
                let base_str = install_path.to_str().ok_or_else(|| {
                    ResolveError::PathConversion(
                        "Cannot convert game install path to string".to_string(),
                    )
                })?;
                result = result.replace("<base>", base_str);
            }

            if needs_game {
                result = result.replace("<game>", &matched_dir);
            }
        }
    }

    // ── Windows-specific variables ──

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

    if result.contains("<winPublic>") {
        let public =
            env::var("PUBLIC").map_err(|_| ResolveError::DirNotFound("PUBLIC".to_string()))?;
        result = result.replace("<winPublic>", &public);
    }

    if result.contains("<winProgramData>") {
        let program_data = env::var("PROGRAMDATA")
            .map_err(|_| ResolveError::DirNotFound("PROGRAMDATA".to_string()))?;
        result = result.replace("<winProgramData>", &program_data);
    }

    if result.contains("<winDir>") {
        let win_dir =
            env::var("WINDIR").map_err(|_| ResolveError::DirNotFound("WINDIR".to_string()))?;
        result = result.replace("<winDir>", &win_dir);
    }

    // ── Linux-specific variables ──

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

    // Resolve <storeUserId>: configured value > auto-detect most recent Steam user
    if result.contains("<storeUserId>") || result.contains("<storeuserid>") {
        let user_id = if let Some(configured) = ctx.and_then(|c| c.store_user_id.as_deref()) {
            configured.to_string()
        } else {
            // Fallback: auto-detect most recently modified user from Steam userdata
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
            user_id
        };

        result = result.replace("<storeUserId>", &user_id);
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
pub(crate) fn get_steam_root() -> Result<String, ResolveError> {
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

    // ── <base> / <game> / <storeGameId> tests ───────────────────────────

    #[test]
    fn test_base_without_context_returns_missing_context() {
        let config = create_test_config();
        let result = resolve_path("<base>/saves", None, &config);
        assert!(matches!(result, Err(ResolveError::MissingContext(_))));
    }

    #[test]
    fn test_game_without_context_returns_missing_context() {
        let config = create_test_config();
        let result = resolve_path("<root>/something/<game>", None, &config);
        // <root> may or may not resolve (depends on Steam), but <game> needs context
        // If <root> fails first, that's also acceptable
        assert!(result.is_err());
    }

    #[test]
    fn test_store_game_id_without_context_returns_missing_context() {
        let config = create_test_config();
        let result = resolve_path("<storeGameId>/saves", None, &config);
        assert!(matches!(result, Err(ResolveError::MissingContext(_))));
    }

    #[test]
    fn test_store_game_id_with_context() {
        let config = create_test_config();
        let ctx = PathContext {
            install_dirs: Vec::new(),
            steam_id: Some(282800),
            install_dir_cache: None,
            game_roots: Vec::new(),
            store_user_id: None,
        };
        let result = resolve_path("/path/<storeGameId>/saves", Some(&ctx), &config);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved, PathBuf::from("/path/282800/saves"));
    }

    #[test]
    fn test_store_game_id_without_steam_id() {
        let config = create_test_config();
        let ctx = PathContext {
            install_dirs: Vec::new(),
            steam_id: None,
            install_dir_cache: None,
            game_roots: Vec::new(),
            store_user_id: None,
        };
        let result = resolve_path("/path/<storeGameId>/saves", Some(&ctx), &config);
        assert!(matches!(result, Err(ResolveError::MissingContext(_))));
    }

    #[test]
    fn test_base_with_context_and_cache() {
        let config = create_test_config();

        // Create temp dir for the game install
        let temp = temp_dir::TempDir::new().unwrap();
        let game_dir = temp.path().join("TestGame");
        std::fs::create_dir_all(&game_dir).unwrap();

        let mut cache = std::collections::HashMap::new();
        cache.insert(
            "testgame".to_string(),
            crate::steam::InstalledSteamGame {
                app_id: 12345,
                name: "Test Game".to_string(),
                install_dir: "TestGame".to_string(),
                install_path: game_dir.clone(),
            },
        );

        let ctx = PathContext {
            install_dirs: vec!["TestGame".to_string()],
            steam_id: Some(12345),
            install_dir_cache: Some(Arc::new(cache)),
            game_roots: Vec::new(),
            store_user_id: None,
        };

        let result = resolve_path("<base>/saves", Some(&ctx), &config);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved, game_dir.join("saves"));
    }

    #[test]
    fn test_game_and_base_same_match() {
        let config = create_test_config();

        let temp = temp_dir::TempDir::new().unwrap();
        let game_dir = temp.path().join("My Game Dir");
        std::fs::create_dir_all(&game_dir).unwrap();

        let mut cache = std::collections::HashMap::new();
        cache.insert(
            "my game dir".to_string(),
            crate::steam::InstalledSteamGame {
                app_id: 99999,
                name: "My Game".to_string(),
                install_dir: "My Game Dir".to_string(),
                install_path: game_dir.clone(),
            },
        );

        let ctx = PathContext {
            install_dirs: vec!["My Game Dir".to_string()],
            steam_id: None,
            install_dir_cache: Some(Arc::new(cache)),
            game_roots: Vec::new(),
            store_user_id: None,
        };

        // A path with both <base> and <game> should resolve consistently
        let result = resolve_path("<base>/<game>/data", Some(&ctx), &config);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved, game_dir.join("My Game Dir").join("data"));
    }

    #[test]
    fn test_base_game_not_installed() {
        let config = create_test_config();
        let cache = std::collections::HashMap::new(); // empty — no games installed

        let ctx = PathContext {
            install_dirs: vec!["NonExistentGame".to_string()],
            steam_id: None,
            install_dir_cache: Some(Arc::new(cache)),
            game_roots: Vec::new(),
            store_user_id: None,
        };

        let result = resolve_path("<base>/saves", Some(&ctx), &config);
        assert!(matches!(result, Err(ResolveError::GameNotInstalled(_))));
    }

    #[test]
    fn test_base_empty_install_dirs() {
        let config = create_test_config();
        let ctx = PathContext {
            install_dirs: Vec::new(),
            steam_id: None,
            install_dir_cache: Some(Arc::new(std::collections::HashMap::new())),
            game_roots: Vec::new(),
            store_user_id: None,
        };

        let result = resolve_path("<base>/saves", Some(&ctx), &config);
        assert!(matches!(result, Err(ResolveError::MissingContext(_))));
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

    #[test]
    fn test_resolve_root_uses_first_configured_game_root() {
        let config = create_test_config();
        let ctx = PathContext {
            game_roots: vec![
                "/mnt/games/steam".to_string(),
                "/mnt/games/steam2".to_string(),
            ],
            ..Default::default()
        };
        let result = resolve_path("<root>/userdata/12345/remote", Some(&ctx), &config).unwrap();
        let result_str = result.to_string_lossy();
        assert!(
            result_str.starts_with("/mnt/games/steam/"),
            "Expected first configured root, got: {result_str}"
        );
        assert!(result_str.ends_with("/userdata/12345/remote"));
    }

    #[test]
    fn test_resolve_root_falls_back_when_game_roots_empty() {
        let config = create_test_config();
        let ctx = PathContext {
            game_roots: vec![],
            ..Default::default()
        };
        // Empty game_roots should fallback to auto-detected Steam root.
        // The call may succeed (if Steam is installed) or fail (if not).
        // Either way, it should NOT panic.
        let _ = resolve_path("<root>/something", Some(&ctx), &config);
    }

    #[test]
    fn test_resolve_root_no_context_falls_back_to_auto_detect() {
        let config = create_test_config();
        // None context = no game_roots = fallback to get_steam_root()
        let _ = resolve_path("<root>/something", None, &config);
    }

    #[test]
    fn test_path_context_from_game_includes_device_game_roots() {
        use crate::backup::Game;
        use crate::device::Device;

        let game = Game {
            name: "Test Game".to_string(),
            storage_key: String::new(),
            save_paths: vec![],
            game_paths: std::collections::HashMap::new(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            ludusavi_meta: None,
            store_user_ids: std::collections::HashMap::new(),
        };
        let device = Device {
            id: "test-device".to_string(),
            name: "Test".to_string(),
            game_roots: vec!["/custom/root".to_string()],
        };

        let ctx = game.path_context(Some(&device));
        assert_eq!(ctx.game_roots, vec!["/custom/root".to_string()]);
    }

    #[test]
    fn test_path_context_from_game_without_device() {
        use crate::backup::Game;

        let game = Game {
            name: "Test Game".to_string(),
            storage_key: String::new(),
            save_paths: vec![],
            game_paths: std::collections::HashMap::new(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            ludusavi_meta: None,
            store_user_ids: std::collections::HashMap::new(),
        };

        let ctx = game.path_context(None);
        assert!(ctx.game_roots.is_empty());
    }

    #[test]
    fn test_resolve_store_user_id_uses_configured_value() {
        let config = create_test_config();
        let ctx = PathContext {
            store_user_id: Some("99887766".to_string()),
            ..Default::default()
        };
        let result = resolve_path(
            "<home>/Steam/userdata/<storeUserId>/saves",
            Some(&ctx),
            &config,
        )
        .unwrap();
        let result_str = result.to_string_lossy();
        assert!(
            result_str.contains("/99887766/"),
            "Expected configured storeUserId, got: {result_str}"
        );
    }

    #[test]
    fn test_path_context_includes_store_user_id_from_game() {
        use crate::backup::Game;
        use crate::device::Device;

        let mut store_user_ids = std::collections::HashMap::new();
        store_user_ids.insert("dev-1".to_string(), "12345678".to_string());

        let game = Game {
            name: "Test Game".to_string(),
            storage_key: String::new(),
            save_paths: vec![],
            game_paths: std::collections::HashMap::new(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            ludusavi_meta: None,
            store_user_ids,
        };
        let device = Device {
            id: "dev-1".to_string(),
            name: "Test".to_string(),
            game_roots: vec![],
        };

        let ctx = game.path_context(Some(&device));
        assert_eq!(ctx.store_user_id, Some("12345678".to_string()));
    }

    #[test]
    fn test_path_context_no_store_user_id_for_unknown_device() {
        use crate::backup::Game;
        use crate::device::Device;

        let mut store_user_ids = std::collections::HashMap::new();
        store_user_ids.insert("dev-1".to_string(), "12345678".to_string());

        let game = Game {
            name: "Test Game".to_string(),
            storage_key: String::new(),
            save_paths: vec![],
            game_paths: std::collections::HashMap::new(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            ludusavi_meta: None,
            store_user_ids,
        };
        let device = Device {
            id: "dev-other".to_string(),
            name: "Other".to_string(),
            game_roots: vec![],
        };

        let ctx = game.path_context(Some(&device));
        assert_eq!(ctx.store_user_id, None);
    }
}
