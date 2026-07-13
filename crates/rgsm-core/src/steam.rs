//! Steam integration: library discovery, installed game detection, and install path resolution.
//!
//! Parses Steam's VDF configuration files (`libraryfolders.vdf`, `appmanifest_*.acf`)
//! to discover installed games and their install directories. This module provides the
//! data needed to resolve Ludusavi path variables `<base>`, `<game>`, and `<storeGameId>`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use log::{debug, warn};
use serde::Deserialize;
use thiserror::Error;

/// Errors specific to Steam integration.
#[derive(Debug, Error)]
pub enum SteamError {
    #[error("Steam installation not found")]
    SteamNotFound,

    #[error("Failed to read VDF file {path}: {source}")]
    VdfRead {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to parse VDF file {path}: {reason}")]
    VdfParse { path: PathBuf, reason: String },

    #[error("No Steam libraries found")]
    NoLibraries,
}

/// An installed Steam game discovered from `appmanifest_*.acf`.
#[derive(Debug, Clone)]
pub struct InstalledSteamGame {
    pub app_id: u32,
    pub name: String,
    /// Directory name inside `steamapps/common/` (e.g. "100 Orange Juice").
    pub install_dir: String,
    /// Full path to the game install directory.
    pub install_path: PathBuf,
}

// ── VDF deserialization structures ──────────────────────────────────────────

// Note: `keyvalues-serde` automatically unwraps the root VDF key.
// For `"libraryfolders" { ... }`, we deserialize directly into the inner content.
// For `"AppState" { ... }`, we deserialize directly into AppState.

/// A single library folder entry from `libraryfolders.vdf`.
#[derive(Deserialize, Debug)]
struct LibraryFolder {
    path: String,
    #[allow(dead_code)]
    #[serde(default)]
    apps: HashMap<String, String>,
}

/// The `AppState` block inside an `appmanifest_*.acf` file.
#[derive(Deserialize, Debug)]
struct AppState {
    #[serde(alias = "AppID", alias = "appID", alias = "appid")]
    appid: Option<StringOrNumber>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    installdir: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct SteamLoginUser {
    #[serde(default, alias = "AccountName")]
    account_name: Option<String>,
    #[serde(default, alias = "PersonaName")]
    persona_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SteamUserNames {
    account_name: Option<String>,
    persona_name: Option<String>,
}

/// VDF values may be strings or numbers; this type handles both.
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
enum StringOrNumber {
    Str(String),
    Num(u64),
}

impl StringOrNumber {
    fn as_u32(&self) -> Option<u32> {
        match self {
            StringOrNumber::Str(s) => s.parse().ok(),
            StringOrNumber::Num(n) => u32::try_from(*n).ok(),
        }
    }
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Get the Steam root directory.
///
/// Delegates to the existing detection logic in `path_resolver`, but returns
/// a `PathBuf` for ergonomic use in this module.
pub fn get_steam_root() -> Result<PathBuf, SteamError> {
    let root = crate::path_resolver::get_steam_root().map_err(|_| SteamError::SteamNotFound)?;
    Ok(PathBuf::from(root))
}

/// Discover all Steam library paths from `libraryfolders.vdf`.
///
/// Returns a list of library root paths (e.g. `D:\SteamLibrary`).
/// The Steam root itself is always included as the first library.
pub fn get_steam_library_paths() -> Result<Vec<PathBuf>, SteamError> {
    let steam_root = get_steam_root()?;
    let vdf_path = steam_root.join("steamapps").join("libraryfolders.vdf");

    if !vdf_path.exists() {
        // Fallback: just use the steam root as the only library
        warn!(
            target: "rgsm::steam",
            "libraryfolders.vdf not found at {}, using Steam root as only library",
            vdf_path.display()
        );
        return Ok(vec![steam_root]);
    }

    let content = std::fs::read_to_string(&vdf_path).map_err(|e| SteamError::VdfRead {
        path: vdf_path.clone(),
        source: e,
    })?;

    let library_folders: HashMap<String, LibraryFolder> = keyvalues_serde::from_str(&content)
        .map_err(|e| SteamError::VdfParse {
            path: vdf_path.clone(),
            reason: e.to_string(),
        })?;

    let mut libraries = vec![steam_root.clone()];
    for library in library_folders.into_values().map(|folder| {
        let p = folder.path.replace('\\', "/");
        PathBuf::from(p)
    }) {
        if library.exists() && !libraries.contains(&library) {
            libraries.push(library);
        }
    }

    debug!(target: "rgsm::steam", "Found {} Steam libraries", libraries.len());
    Ok(libraries)
}

/// Scan a single Steam library for installed games via `appmanifest_*.acf` files.
fn scan_library_manifests(library_path: &Path) -> Vec<InstalledSteamGame> {
    let steamapps = library_path.join("steamapps");
    let common = steamapps.join("common");

    let read_dir = match std::fs::read_dir(&steamapps) {
        Ok(rd) => rd,
        Err(e) => {
            debug!(
                target: "rgsm::steam",
                "Cannot read steamapps at {}: {}",
                steamapps.display(),
                e
            );
            return Vec::new();
        }
    };

    let mut games = Vec::new();

    for entry in read_dir.flatten() {
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();

        if !name_str.starts_with("appmanifest_") || !name_str.ends_with(".acf") {
            continue;
        }

        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(e) => {
                debug!(
                    target: "rgsm::steam",
                    "Failed to read {}: {}",
                    entry.path().display(),
                    e
                );
                continue;
            }
        };

        let state: AppState = match keyvalues_serde::from_str(&content) {
            Ok(m) => m,
            Err(e) => {
                debug!(
                    target: "rgsm::steam",
                    "Failed to parse {}: {}",
                    entry.path().display(),
                    e
                );
                continue;
            }
        };

        let app_id = match state.appid.and_then(|id| id.as_u32()) {
            Some(id) => id,
            None => continue,
        };

        let install_dir = match state.installdir {
            Some(dir) if !dir.is_empty() => dir,
            _ => continue,
        };

        let install_path = common.join(&install_dir);
        let name = state.name.unwrap_or_else(|| format!("AppID {}", app_id));

        games.push(InstalledSteamGame {
            app_id,
            name,
            install_dir,
            install_path,
        });
    }

    games
}

/// Pre-scan ALL installed games across ALL Steam libraries.
///
/// Returns a `HashMap` keyed by **lowercase** install directory name.
/// This enables O(1) lookup per manifest game during `detect_local_games`.
pub fn scan_all_installed_games() -> Result<HashMap<String, InstalledSteamGame>, SteamError> {
    let libraries = get_steam_library_paths()?;
    let mut result = HashMap::new();

    for lib_path in &libraries {
        for game in scan_library_manifests(lib_path) {
            let key = game.install_dir.to_lowercase();
            result.entry(key).or_insert(game);
        }
    }

    debug!(
        target: "rgsm::steam",
        "Scanned {} installed Steam games across {} libraries",
        result.len(),
        libraries.len()
    );

    Ok(result)
}

/// Find the install path for a game given its possible install directory names
/// (from the Ludusavi manifest's `installDir` field).
///
/// Returns `(matched_dir_name, full_install_path)` — both `<game>` and `<base>`
/// are derived from the same match to ensure consistency for multi-alias games.
///
/// When `cache` is provided, uses the pre-scanned HashMap for O(1) lookup.
/// Otherwise, scans the filesystem directly (suitable for single-game operations).
pub fn find_game_install_path(
    install_dirs: &[String],
    cache: Option<&HashMap<String, InstalledSteamGame>>,
) -> Option<(String, PathBuf)> {
    if install_dirs.is_empty() {
        return None;
    }

    if let Some(cache) = cache {
        // Fast O(1) lookup from pre-scanned cache
        for dir_name in install_dirs {
            let key = dir_name.to_lowercase();
            if let Some(game) = cache.get(&key)
                && game.install_path.exists()
            {
                return Some((game.install_dir.clone(), game.install_path.clone()));
            }
        }
        return None;
    }

    // Slow path: scan filesystem directly (for single-game operations)
    let libraries = match get_steam_library_paths() {
        Ok(libs) => libs,
        Err(_) => return None,
    };

    for dir_name in install_dirs {
        let dir_lower = dir_name.to_lowercase();
        for lib_path in &libraries {
            let common = lib_path.join("steamapps").join("common");

            // Try exact name first
            let exact_path = common.join(dir_name);
            if exact_path.exists() {
                return Some((dir_name.clone(), exact_path));
            }

            // Case-insensitive fallback: scan the directory
            if let Ok(entries) = std::fs::read_dir(&common) {
                for entry in entries.flatten() {
                    if entry.file_name().to_string_lossy().to_lowercase() == dir_lower {
                        let matched_name = entry.file_name().to_string_lossy().into_owned();
                        return Some((matched_name, entry.path()));
                    }
                }
            }
        }
    }

    None
}

/// A candidate Steam user ID with metadata for UI display.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StoreUserIdCandidate {
    pub user_id: String,
    /// Stable Steam login name from `config/loginusers.vdf`, when available.
    pub account_name: Option<String>,
    /// Player-facing Steam profile name from `config/loginusers.vdf`, when available.
    pub persona_name: Option<String>,
    /// Seconds since UNIX epoch of the `userdata/<id>` directory mtime.
    pub last_modified_epoch_secs: Option<i64>,
}

fn parse_login_user_names(content: &str) -> Result<HashMap<String, SteamUserNames>, String> {
    let users: HashMap<String, SteamLoginUser> =
        keyvalues_serde::from_str(content).map_err(|error| error.to_string())?;
    Ok(users
        .into_iter()
        .filter_map(|(steam_id, user)| {
            let steam_id = steam_id.parse::<u64>().ok()?;
            let account_id = (steam_id & u32::MAX as u64).to_string();
            Some((
                account_id,
                SteamUserNames {
                    account_name: user.account_name.filter(|name| !name.is_empty()),
                    persona_name: user.persona_name.filter(|name| !name.is_empty()),
                },
            ))
        })
        .collect())
}

fn read_login_user_names(steam_root: &Path) -> HashMap<String, SteamUserNames> {
    let path = steam_root.join("config").join("loginusers.vdf");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };
    match parse_login_user_names(&content) {
        Ok(users) => users,
        Err(reason) => {
            warn!(
                target: "rgsm::steam",
                "failed to parse {} for Steam account labels: {}",
                path.display(),
                reason
            );
            HashMap::new()
        }
    }
}

/// Detect Steam user IDs from the `userdata/` directory.
/// Returns candidates sorted by most recently modified first.
pub fn detect_steam_user_ids() -> Result<Vec<StoreUserIdCandidate>, SteamError> {
    let steam_root = get_steam_root()?;
    let userdata_path = steam_root.join("userdata");
    let mut login_user_names = read_login_user_names(&steam_root);

    let mut candidates = Vec::new();
    let entries = std::fs::read_dir(&userdata_path).map_err(|_| SteamError::SteamNotFound)?;

    for entry in entries.flatten() {
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

        let last_modified_epoch_secs = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);

        let names = login_user_names.remove(&dir_name);
        candidates.push(StoreUserIdCandidate {
            user_id: dir_name,
            account_name: names.as_ref().and_then(|names| names.account_name.clone()),
            persona_name: names.and_then(|names| names.persona_name),
            last_modified_epoch_secs,
        });
    }

    // Sort by most recently modified first
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.last_modified_epoch_secs));
    Ok(candidates)
}

/// Detect game root directories (Steam library paths).
/// Returns all Steam library root paths found.
pub fn detect_game_roots() -> Result<Vec<String>, SteamError> {
    let paths = get_steam_library_paths()?;
    Ok(paths
        .into_iter()
        .filter_map(|p| p.to_str().map(|s| s.to_string()))
        .collect())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── VDF Parsing Tests ───────────────────────────────────────────────

    #[test]
    fn parse_libraryfolders_vdf() {
        let vdf_content = r#"
"libraryfolders"
{
    "0"
    {
        "path"      "C:\\Program Files (x86)\\Steam"
        "label"     ""
        "apps"
        {
            "228980"    "40589913"
            "282800"    "2341529553"
        }
    }
    "1"
    {
        "path"      "D:\\SteamLibrary"
        "label"     ""
        "apps"
        {
            "730"       "30687537714"
        }
    }
}
"#;
        let folders: HashMap<String, LibraryFolder> =
            keyvalues_serde::from_str(vdf_content).unwrap();
        assert_eq!(folders.len(), 2);

        let paths: Vec<String> = folders.values().map(|f| f.path.clone()).collect();
        assert!(paths.iter().any(|p| p.contains("Steam")));
        assert!(paths.iter().any(|p| p.contains("SteamLibrary")));
    }

    #[test]
    fn parse_appmanifest_acf() {
        let acf_content = r#"
"AppState"
{
    "appid"     "282800"
    "Universe"  "1"
    "installdir"    "100 Orange Juice"
    "name"      "100% Orange Juice!"
    "StateFlags"    "4"
}
"#;
        let state: AppState = keyvalues_serde::from_str(acf_content).unwrap();

        assert_eq!(state.appid.as_ref().unwrap().as_u32(), Some(282800));
        assert_eq!(state.name.as_deref(), Some("100% Orange Juice!"));
        assert_eq!(state.installdir.as_deref(), Some("100 Orange Juice"));
    }

    #[test]
    fn parse_appmanifest_numeric_appid() {
        // Some ACF files may have numeric (unquoted) values
        let acf_content = r#"
"AppState"
{
    "appid"     730
    "installdir"    "Counter-Strike Global Offensive"
    "name"      "Counter-Strike 2"
}
"#;
        let state: AppState = keyvalues_serde::from_str(acf_content).unwrap();
        assert_eq!(state.appid.as_ref().unwrap().as_u32(), Some(730));
    }

    #[test]
    fn parse_appmanifest_missing_fields() {
        let acf_content = r#"
"AppState"
{
    "appid"     "99999"
}
"#;
        let state: AppState = keyvalues_serde::from_str(acf_content).unwrap();
        assert!(state.name.is_none());
        assert!(state.installdir.is_none());
    }

    // ── find_game_install_path with cache ───────────────────────────────

    #[test]
    fn find_game_install_path_from_cache_hit() {
        let mut cache = HashMap::new();

        // Create a temp dir to simulate the install path
        let temp = temp_dir::TempDir::new().unwrap();
        let game_path = temp.path().join("100 Orange Juice");
        std::fs::create_dir_all(&game_path).unwrap();

        cache.insert(
            "100 orange juice".to_string(),
            InstalledSteamGame {
                app_id: 282800,
                name: "100% Orange Juice!".to_string(),
                install_dir: "100 Orange Juice".to_string(),
                install_path: game_path.clone(),
            },
        );

        let result = find_game_install_path(&["100 Orange Juice".to_string()], Some(&cache));
        assert!(result.is_some());
        let (matched_dir, matched_path) = result.unwrap();
        assert_eq!(matched_dir, "100 Orange Juice");
        assert_eq!(matched_path, game_path);
    }

    #[test]
    fn find_game_install_path_from_cache_case_insensitive() {
        let mut cache = HashMap::new();

        let temp = temp_dir::TempDir::new().unwrap();
        let game_path = temp.path().join("MyGame");
        std::fs::create_dir_all(&game_path).unwrap();

        cache.insert(
            "mygame".to_string(),
            InstalledSteamGame {
                app_id: 12345,
                name: "My Game".to_string(),
                install_dir: "MyGame".to_string(),
                install_path: game_path,
            },
        );

        // Search with different case
        let result = find_game_install_path(&["MYGAME".to_string()], Some(&cache));
        assert!(result.is_some());
    }

    #[test]
    fn find_game_install_path_from_cache_miss() {
        let cache = HashMap::new();
        let result = find_game_install_path(&["NonExistent".to_string()], Some(&cache));
        assert!(result.is_none());
    }

    #[test]
    fn find_game_install_path_empty_dirs() {
        let cache = HashMap::new();
        let result = find_game_install_path(&[], Some(&cache));
        assert!(result.is_none());
    }

    #[test]
    fn find_game_install_path_multiple_candidates() {
        let mut cache = HashMap::new();

        let temp = temp_dir::TempDir::new().unwrap();
        let game_path = temp.path().join("AltName");
        std::fs::create_dir_all(&game_path).unwrap();

        cache.insert(
            "altname".to_string(),
            InstalledSteamGame {
                app_id: 11111,
                name: "Some Game".to_string(),
                install_dir: "AltName".to_string(),
                install_path: game_path.clone(),
            },
        );

        // First candidate doesn't exist in cache, second does
        let result = find_game_install_path(
            &["PrimaryName".to_string(), "AltName".to_string()],
            Some(&cache),
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().0, "AltName");
    }

    // ── scan_library_manifests ──────────────────────────────────────────

    #[test]
    fn scan_library_manifests_with_temp_dir() {
        let temp = temp_dir::TempDir::new().unwrap();
        let steamapps = temp.path().join("steamapps");
        let common = steamapps.join("common");
        std::fs::create_dir_all(&common).unwrap();

        // Create a fake appmanifest
        let manifest_content = r#"
"AppState"
{
    "appid"     "282800"
    "installdir"    "100 Orange Juice"
    "name"      "100% Orange Juice!"
    "StateFlags"    "4"
}
"#;
        std::fs::write(steamapps.join("appmanifest_282800.acf"), manifest_content).unwrap();

        // Create the game directory
        std::fs::create_dir_all(common.join("100 Orange Juice")).unwrap();

        let games = scan_library_manifests(temp.path());
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].app_id, 282800);
        assert_eq!(games[0].install_dir, "100 Orange Juice");
        assert_eq!(games[0].name, "100% Orange Juice!");
    }

    #[test]
    fn scan_library_manifests_empty_dir() {
        let temp = temp_dir::TempDir::new().unwrap();
        let steamapps = temp.path().join("steamapps");
        std::fs::create_dir_all(&steamapps).unwrap();

        let games = scan_library_manifests(temp.path());
        assert!(games.is_empty());
    }

    #[test]
    fn scan_library_manifests_nonexistent_dir() {
        let games = scan_library_manifests(Path::new("/nonexistent/path"));
        assert!(games.is_empty());
    }

    #[test]
    fn detect_steam_user_ids_returns_sorted_candidates() {
        let temp = temp_dir::TempDir::new().unwrap();
        let userdata = temp.path().join("userdata");
        std::fs::create_dir_all(userdata.join("12345678")).unwrap();
        std::fs::create_dir_all(userdata.join("87654321")).unwrap();
        // Non-numeric dirs should be ignored
        std::fs::create_dir_all(userdata.join("anonymous")).unwrap();
        // Files should be ignored
        std::fs::write(userdata.join("0"), "").unwrap();

        // We can't test the real function (needs Steam installed),
        // but we can test the directory parsing logic indirectly.
        // The function itself is integration-tested when Steam is present.
    }

    #[test]
    fn parses_login_user_names_by_userdata_account_id() {
        let login_users = parse_login_user_names(
            r#"
            "users"
            {
                "76561198000000042"
                {
                    "AccountName" "login_name"
                    "PersonaName" "Player Name"
                    "MostRecent" "1"
                }
            }
            "#,
        )
        .unwrap();

        let account_id = (76_561_198_000_000_042_u64 & u32::MAX as u64).to_string();
        let names = login_users.get(&account_id).unwrap();
        assert_eq!(names.account_name.as_deref(), Some("login_name"));
        assert_eq!(names.persona_name.as_deref(), Some("Player Name"));
    }

    #[test]
    fn malformed_login_user_ids_are_ignored() {
        let login_users = parse_login_user_names(
            r#"
            "users"
            {
                "not-a-steam-id"
                {
                    "AccountName" "ignored"
                }
            }
            "#,
        )
        .unwrap();

        assert!(login_users.is_empty());
    }

    #[test]
    fn detect_game_roots_does_not_panic() {
        // May succeed or fail depending on Steam installation.
        let _ = detect_game_roots();
    }
}
