use crate::{
    app_dirs,
    config::Config,
    embedded_resources,
    path_resolver::{self, PathContext},
    steam,
};
use anyhow::{Context, Result};
use chrono::Utc;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// URL to the ludusavi manifest raw YAML file (used only for user-triggered updates).
const MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml";

/// Local manifest cache filename in app data dir.
const LOCAL_MANIFEST_FILENAME: &str = "ludusavi_manifest.yaml";
const LOCAL_MANIFEST_META_FILENAME: &str = "ludusavi_manifest.meta.json";

/// Cache duration for the manifest (1 hour)
const CACHE_DURATION: Duration = Duration::from_secs(3600);

/// Cached manifest data with timestamp
struct ManifestCache {
    data: HashMap<String, serde_yaml::Value>,
    fetched_at: Instant,
}

// Global cache for the manifest
lazy_static::lazy_static! {
    static ref MANIFEST_CACHE: Arc<Mutex<Option<ManifestCache>>> = Arc::new(Mutex::new(None));
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LudusaviManifestStatus {
    /// Current manifest source: `local`, `bundled`, or `none`.
    pub source: String,
    /// Last update time (RFC3339, best-effort).
    pub updated_at: Option<String>,
    /// A version-like identifier (best-effort, e.g. HTTP ETag).
    pub etag: Option<String>,
    /// Whether a local cached manifest exists.
    pub has_local: bool,
    /// Local cache path (if available as a string).
    pub local_path: Option<String>,
    /// Local cache size in bytes (if present).
    pub local_bytes: Option<u64>,
    /// Bundled manifest size in bytes.
    pub bundled_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestMeta {
    updated_at: Option<String>,
    etag: Option<String>,
    source_url: Option<String>,
}

/// Represents a single game entry in the ludusavi manifest
/// Currently unused but kept for future expansions
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ManifestGame {
    /// Game name (used as the key in the manifest)
    pub name: String,
    /// Steam ID if available
    pub steam_id: Option<u32>,
    /// Save file paths with platform conditions
    pub files: Vec<SavePath>,
    /// Registry paths (Windows only)
    pub registry: Vec<String>,
}

/// Represents a save file path with its conditions
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SavePath {
    /// The path pattern
    pub path: String,
    /// Tags like "save", "config", etc.
    pub tags: Vec<String>,
}

/// Simplified game info for the import dialog
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportableGame {
    /// Game name
    pub name: String,
    /// Steam ID if available
    pub steam_id: Option<u32>,
    /// Install directory names from manifest's `installDir` field
    pub install_dirs: Vec<String>,
    /// Whether this game is already managed
    pub is_managed: bool,
    /// Number of save paths detected
    pub save_paths_count: usize,
}

/// Downloads and parses the ludusavi manifest with caching
/// Note: Manifest is loaded from local cache if present, otherwise from the bundled snapshot.
pub async fn fetch_manifest() -> Result<HashMap<String, serde_yaml::Value>> {
    // Check cache first
    {
        let cache = MANIFEST_CACHE.lock().unwrap();
        if let Some(cached) = cache.as_ref() {
            if cached.fetched_at.elapsed() < CACHE_DURATION {
                debug!(target: "rgsm::ludusavi", "Using cached manifest with {} games", cached.data.len());
                return Ok(cached.data.clone());
            }
        }
    }

    // Load manifest YAML (offline-first)
    let text = load_manifest_yaml().context("Failed to load Ludusavi manifest YAML")?;

    let manifest: HashMap<String, serde_yaml::Value> =
        serde_yaml::from_str(&text).context("Failed to parse manifest YAML")?;

    info!(target: "rgsm::ludusavi", "Successfully parsed manifest with {} games", manifest.len());

    // Update cache
    {
        let mut cache = MANIFEST_CACHE.lock().unwrap();
        *cache = Some(ManifestCache {
            data: manifest.clone(),
            fetched_at: Instant::now(),
        });
    }

    Ok(manifest)
}

pub fn get_manifest_status() -> LudusaviManifestStatus {
    let local_path = app_dirs::resolve_app_path(LOCAL_MANIFEST_FILENAME);
    let meta_path = app_dirs::resolve_app_path(LOCAL_MANIFEST_META_FILENAME);
    let has_local = local_path.exists();
    let has_bundled = embedded_resources::has_bundled_manifest();
    let local_bytes = if has_local {
        fs::metadata(&local_path).ok().map(|m| m.len())
    } else {
        None
    };
    let local_path_str = local_path.to_str().map(|s| s.to_string());

    let (updated_at, etag) = if has_local {
        let meta = fs::read_to_string(&meta_path)
            .ok()
            .and_then(|s| serde_json::from_str::<ManifestMeta>(&s).ok());
        let updated_at = meta
            .as_ref()
            .and_then(|m| m.updated_at.clone())
            .or_else(|| {
                fs::metadata(&local_path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
            });
        let etag = meta.and_then(|m| m.etag);
        (updated_at, etag)
    } else if has_bundled {
        let meta = serde_json::from_str::<ManifestMeta>(
            embedded_resources::ludusavi_manifest_meta_json().as_ref(),
        )
        .ok();
        let updated_at = meta.as_ref().and_then(|m| m.updated_at.clone());
        let etag = meta.and_then(|m| m.etag);
        (updated_at, etag)
    } else {
        (None, None)
    };

    LudusaviManifestStatus {
        source: if has_local {
            "local".to_string()
        } else if has_bundled {
            "bundled".to_string()
        } else {
            "none".to_string()
        },
        updated_at,
        etag,
        has_local,
        local_path: local_path_str,
        local_bytes,
        bundled_bytes: embedded_resources::ludusavi_manifest_yaml_len(),
    }
}

pub async fn update_manifest_from_remote() -> Result<LudusaviManifestStatus> {
    info!(target: "rgsm::ludusavi", "Downloading Ludusavi manifest from {}", MANIFEST_URL);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .get(MANIFEST_URL)
        .send()
        .await
        .context("Failed to download manifest")?
        .error_for_status()
        .context("Manifest download returned an error status")?;

    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let text = response
        .text()
        .await
        .context("Failed to read manifest text")?;

    // Validate YAML before writing.
    let _: HashMap<String, serde_yaml::Value> =
        serde_yaml::from_str(&text).context("Downloaded manifest is not valid YAML")?;

    let local_path = app_dirs::resolve_app_path(LOCAL_MANIFEST_FILENAME);
    fs::write(&local_path, text).with_context(|| {
        format!(
            "Failed to write local manifest cache to {}",
            local_path.display()
        )
    })?;

    let meta_path = app_dirs::resolve_app_path(LOCAL_MANIFEST_META_FILENAME);
    let meta = ManifestMeta {
        updated_at: Some(Utc::now().to_rfc3339()),
        etag,
        source_url: Some(MANIFEST_URL.to_string()),
    };
    if let Ok(meta_json) = serde_json::to_string_pretty(&meta) {
        if let Err(e) = fs::write(&meta_path, meta_json) {
            warn!(
                target: "rgsm::ludusavi",
                "Failed to write manifest meta cache at {}: {}",
                meta_path.display(),
                e
            );
        }
    }

    // Invalidate in-memory cache so future calls re-load.
    {
        let mut cache = MANIFEST_CACHE.lock().unwrap();
        *cache = None;
    }

    Ok(get_manifest_status())
}

pub fn reset_manifest_to_bundled() -> Result<LudusaviManifestStatus> {
    let local_path = app_dirs::resolve_app_path(LOCAL_MANIFEST_FILENAME);
    if local_path.exists() {
        fs::remove_file(&local_path).with_context(|| {
            format!(
                "Failed to remove local manifest cache at {}",
                local_path.display()
            )
        })?;
    }

    let meta_path = app_dirs::resolve_app_path(LOCAL_MANIFEST_META_FILENAME);
    if meta_path.exists() {
        fs::remove_file(&meta_path).with_context(|| {
            format!(
                "Failed to remove local manifest meta cache at {}",
                meta_path.display()
            )
        })?;
    }

    {
        let mut cache = MANIFEST_CACHE.lock().unwrap();
        *cache = None;
    }

    Ok(get_manifest_status())
}

fn load_manifest_yaml() -> Result<Cow<'static, str>> {
    let local_path = app_dirs::resolve_app_path(LOCAL_MANIFEST_FILENAME);
    if local_path.exists() {
        let text = fs::read_to_string(&local_path).with_context(|| {
            format!(
                "Failed to read local manifest cache at {}",
                local_path.display()
            )
        })?;
        info!(
            target: "rgsm::ludusavi",
            "Using local Ludusavi manifest cache at {} ({} bytes)",
            local_path.display(),
            text.len()
        );
        return Ok(Cow::Owned(text));
    }

    if embedded_resources::has_bundled_manifest() {
        info!(
            target: "rgsm::ludusavi",
            "Using bundled Ludusavi manifest snapshot ({} bytes)",
            embedded_resources::ludusavi_manifest_yaml_len()
        );
        return Ok(embedded_resources::ludusavi_manifest_yaml());
    }

    Err(anyhow::anyhow!(
        "No local Ludusavi manifest cache found. This slim build does not include a bundled snapshot — please update the manifest once while online."
    ))
}

/// Checks if a path's `when` conditions match the current system
fn matches_current_system(when_value: Option<&serde_yaml::Value>) -> bool {
    // If no when conditions, it applies to all systems
    let Some(when_array) = when_value.and_then(|v| v.as_sequence()) else {
        return true;
    };

    // Current OS
    #[cfg(target_os = "windows")]
    let current_os = "windows";
    #[cfg(target_os = "linux")]
    let current_os = "linux";
    #[cfg(target_os = "macos")]
    let current_os = "mac";

    // Check if any condition matches
    for condition in when_array {
        if let Some(os) = condition.get("os").and_then(|o| o.as_str()) {
            if os == current_os {
                return true;
            }
        } else {
            // No OS specified in this condition, matches all
            return true;
        }
    }

    false
}

/// Detects locally installed games by scanning for existing save paths
/// Only considers paths that match the current OS
/// Extract `installDir` names from a manifest game entry.
///
/// The manifest uses this format:
/// ```yaml
/// installDir:
///   100 Orange Juice: {}
///   AltName: {}
/// ```
pub fn extract_install_dirs(value: &serde_yaml::Value) -> Vec<String> {
    value
        .get("installDir")
        .and_then(|v| v.as_mapping())
        .map(|m| {
            m.keys()
                .filter_map(|k| k.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Extract Steam app ID from a manifest game entry.
///
/// The manifest uses this format:
/// ```yaml
/// steam:
///   id: 282800
/// ```
pub fn extract_steam_id(value: &serde_yaml::Value) -> Option<u32> {
    value
        .get("steam")
        .and_then(|s| s.get("id"))
        .and_then(|id| id.as_u64())
        .map(|id| id as u32)
}

pub fn detect_local_games(
    manifest: &HashMap<String, serde_yaml::Value>,
    config: &Config,
) -> HashSet<String> {
    let mut detected = HashSet::new();

    info!(target: "rgsm::ludusavi", "Scanning for locally installed games on current OS...");

    // Pre-scan all installed Steam games for O(1) per-game lookup
    let steam_cache = Arc::new(steam::scan_all_installed_games().unwrap_or_default());

    for (name, value) in manifest {
        let install_dirs = extract_install_dirs(value);
        let steam_id = extract_steam_id(value);

        let ctx = PathContext {
            install_dirs,
            steam_id,
            install_dir_cache: Some(Arc::clone(&steam_cache)),
            game_roots: Vec::new(),
            store_user_id: None,
        };

        // Check if any save paths exist locally
        if let Some(files) = value.get("files").and_then(|f| f.as_mapping()) {
            for (path_key, path_value) in files {
                if let Some(path_str) = path_key.as_str() {
                    // Check if this path applies to current OS
                    let when_conditions = path_value.get("when");
                    if !matches_current_system(when_conditions) {
                        continue;
                    }

                    // Skip registry paths on non-Windows
                    #[cfg(not(target_os = "windows"))]
                    if path_str.starts_with("HKEY_") || path_str.contains("REGISTRY:") {
                        continue;
                    }

                    // Try to resolve the path with game context
                    match path_resolver::resolve_path(path_str, Some(&ctx), config) {
                        Ok(resolved) if resolved.exists() => {
                            detected.insert(name.clone());
                            debug!(target: "rgsm::ludusavi", "Detected installed game: {} (path: {})", name, path_str);
                            break;
                        }
                        // Game not installed on this machine — skip silently
                        Err(path_resolver::ResolveError::GameNotInstalled(_))
                        | Err(path_resolver::ResolveError::StoreNotSupported(_)) => continue,
                        _ => continue,
                    }
                }
            }
        }

        // Also check registry paths on Windows
        #[cfg(target_os = "windows")]
        if let Some(registry) = value.get("registry").and_then(|r| r.as_mapping()) {
            for (path_key, path_value) in registry {
                if let Some(_path_str) = path_key.as_str() {
                    // Check if this path applies to current OS
                    let when_conditions = path_value.get("when");
                    if !matches_current_system(when_conditions) {
                        continue;
                    }

                    // For registry, we could check if the key exists
                    // For now, skip registry detection as it's complex
                    // TODO: Implement Windows registry checking
                }
            }
        }
    }

    info!(target: "rgsm::ludusavi", "Detected {} locally installed games", detected.len());
    detected
}

/// Converts raw manifest data to a list of importable games, optionally filtered by local detection
pub fn parse_manifest_games(
    manifest: &HashMap<String, serde_yaml::Value>,
    managed_games: &[String],
    filter_local_only: bool,
    config: &Config,
) -> Vec<ImportableGame> {
    // Create a HashSet of lowercase managed game names for O(1) lookups
    let managed_set: HashSet<String> = managed_games.iter().map(|g| g.to_lowercase()).collect();

    // Detect local games if filtering is enabled
    let local_games = if filter_local_only {
        Some(detect_local_games(manifest, config))
    } else {
        None
    };

    let mut games = Vec::new();

    for (name, value) in manifest {
        // If filtering by local games, skip games not detected locally
        if let Some(ref local) = local_games {
            if !local.contains(name) {
                continue;
            }
        }

        // Extract Steam ID if available
        let steam_id = extract_steam_id(value);

        // Extract install directory names
        let install_dirs = extract_install_dirs(value);

        // Count save paths
        let save_paths_count = count_save_paths(value);

        // Check if already managed (case-insensitive) using O(1) lookup
        let is_managed = managed_set.contains(&name.to_lowercase());

        games.push(ImportableGame {
            name: name.clone(),
            steam_id,
            install_dirs,
            is_managed,
            save_paths_count,
        });
    }

    // Sort: unmanaged first, then alphabetically
    games.sort_by(|a, b| match (a.is_managed, b.is_managed) {
        (false, true) => std::cmp::Ordering::Less,
        (true, false) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    games
}

/// Counts the number of save paths in a game manifest entry
fn count_save_paths(value: &serde_yaml::Value) -> usize {
    let mut count = 0;

    // Count file paths
    if let Some(files) = value.get("files").and_then(|f| f.as_mapping()) {
        count += files.len();
    }

    // Count registry paths
    if let Some(registry) = value.get("registry").and_then(|r| r.as_mapping()) {
        count += registry.len();
    }

    count
}

/// Extracts save paths from a game's manifest entry, filtered by current OS
pub fn extract_save_paths(game_name: &str, value: &serde_yaml::Value) -> Result<Vec<SavePath>> {
    let mut paths = Vec::new();

    // Extract file paths
    if let Some(files) = value.get("files").and_then(|f| f.as_mapping()) {
        for (path_key, path_value) in files {
            if let Some(path_str) = path_key.as_str() {
                // Check if this path applies to current OS
                let when_conditions = path_value.get("when");
                if !matches_current_system(when_conditions) {
                    continue;
                }

                // Skip registry paths on non-Windows
                #[cfg(not(target_os = "windows"))]
                if path_str.starts_with("HKEY_") {
                    continue;
                }

                let tags = extract_tags(path_value);
                paths.push(SavePath {
                    path: path_str.to_string(),
                    tags,
                });
            }
        }
    }

    // Extract registry paths (for Windows only)
    #[cfg(target_os = "windows")]
    if let Some(registry) = value.get("registry").and_then(|r| r.as_mapping()) {
        for (path_key, path_value) in registry {
            if let Some(path_str) = path_key.as_str() {
                // Check if this path applies to current OS
                let when_conditions = path_value.get("when");
                if !matches_current_system(when_conditions) {
                    continue;
                }

                let tags = extract_tags(path_value);
                paths.push(SavePath {
                    path: format!("REGISTRY:{}", path_str),
                    tags,
                });
            }
        }
    }

    if paths.is_empty() {
        warn!(target: "rgsm::ludusavi", "No save paths found for game: {} on current OS", game_name);
    }

    Ok(paths)
}

/// Extracts tags from a path value
fn extract_tags(value: &serde_yaml::Value) -> Vec<String> {
    value
        .get("tags")
        .and_then(|t| t.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_save_paths() {
        let yaml = r#"
files:
  "<winLocalAppData>/anyway":
    tags:
      - save
  "<winDocuments>/saves":
    tags:
      - save
registry:
  "HKEY_CURRENT_USER/SOFTWARE/Game":
    tags:
      - save
"#;
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(count_save_paths(&value), 3);
    }

    #[test]
    fn test_extract_save_paths_binding_of_isaac() {
        // Test with The Binding of Isaac paths from the manifest
        let yaml = r#"
files:
  "<home>/Library/Application Support/Binding of Isaac Rebirth":
    tags:
      - save
  "<windocuments>/My Games/Binding of Isaac Rebirth":
    tags:
      - save
  "<xdgdata>/binding of isaac rebirth":
    tags:
      - save
"#;
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let paths = extract_save_paths("The Binding of Isaac: Rebirth", &value).unwrap();

        assert_eq!(paths.len(), 3);
        assert!(paths.iter().any(|p| p.path.contains("<home>")));
        assert!(paths.iter().any(|p| p.path.contains("<windocuments>")));
        assert!(paths.iter().any(|p| p.path.contains("<xdgdata>")));

        for path in &paths {
            assert_eq!(path.tags, vec!["save"]);
        }
    }

    #[test]
    fn test_extract_save_paths_with_storeuserid() {
        // Test with Age of Empires paths that include <storeuserid>
        let yaml = r#"
files:
  "<root>/userdata/<storeuserid>/221380/remote":
    tags:
      - save
  "<home>/Games/Age of Empires 2 DE/<storeuserid>/savegame":
    tags:
      - save
"#;
        let value: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let paths = extract_save_paths("Age of Empires II: Definitive Edition", &value).unwrap();

        assert_eq!(paths.len(), 2);
        assert!(paths.iter().any(|p| p.path.contains("<root>")));
        assert!(paths.iter().any(|p| p.path.contains("<storeuserid>")));
    }

    #[test]
    fn test_parse_manifest_games_filtering() {
        let mut manifest = HashMap::new();

        // Add a test game
        let game_yaml = r#"
steam:
  id: 12345
files:
  "<winDocuments>/TestGame":
    tags:
      - save
"#;
        manifest.insert(
            "Test Game".to_string(),
            serde_yaml::from_str(game_yaml).unwrap(),
        );

        let managed_games = vec![];

        // Test without filtering
        let all_games = parse_manifest_games(&manifest, &managed_games, false, &Config::default());
        assert_eq!(all_games.len(), 1);
        assert_eq!(all_games[0].name, "Test Game");
        assert_eq!(all_games[0].steam_id, Some(12345));
        assert!(!all_games[0].is_managed);
    }

    #[test]
    fn test_parse_manifest_games_managed_detection() {
        let mut manifest = HashMap::new();

        let game_yaml = r#"
files:
  "<winDocuments>/TestGame":
    tags:
      - save
"#;
        manifest.insert(
            "Test Game".to_string(),
            serde_yaml::from_str(game_yaml).unwrap(),
        );

        let managed_games = vec!["Test Game".to_string()];

        let games = parse_manifest_games(&manifest, &managed_games, false, &Config::default());
        assert_eq!(games.len(), 1);
        assert!(games[0].is_managed);
    }
}
