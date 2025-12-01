use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SteamDetectorError {
    #[error("Steam installation not found")]
    SteamNotFound,
    #[error("Failed to read file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to parse VDF: {0}")]
    VdfParseError(String),
    #[error("Failed to download manifest: {0}")]
    ManifestDownloadError(String),
    #[error("Failed to parse manifest: {0}")]
    ManifestParseError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SteamGame {
    pub app_id: String,
    pub name: String,
    pub install_dir: String,
    pub install_path: PathBuf,
    pub save_paths: Vec<SteamSavePath>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SteamSavePath {
    pub path: String,
    pub path_type: String, // "file" or "directory"
}

/// Detects the Steam installation directory
pub fn detect_steam_path() -> Result<PathBuf, SteamDetectorError> {
    #[cfg(target_os = "windows")]
    {
        // Try common Steam installation paths on Windows
        let program_files = std::env::var("ProgramFiles(x86)")
            .unwrap_or_else(|_| "C:\\Program Files (x86)".to_string());
        let steam_path = PathBuf::from(program_files).join("Steam");
        
        if steam_path.exists() {
            return Ok(steam_path);
        }

        // Try default location
        let default_path = PathBuf::from("C:\\Program Files (x86)\\Steam");
        if default_path.exists() {
            return Ok(default_path);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().ok_or(SteamDetectorError::SteamNotFound)?;
        let steam_path = home.join("Library/Application Support/Steam");
        if steam_path.exists() {
            return Ok(steam_path);
        }
    }

    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().ok_or(SteamDetectorError::SteamNotFound)?;
        
        // Try ~/.steam/steam (most common)
        let steam_path = home.join(".steam/steam");
        if steam_path.exists() {
            return Ok(steam_path);
        }

        // Try ~/.local/share/Steam (Flatpak)
        let flatpak_path = home.join(".local/share/Steam");
        if flatpak_path.exists() {
            return Ok(flatpak_path);
        }

        // Try snap location
        let snap_path = home.join("snap/steam/common/.steam/steam");
        if snap_path.exists() {
            return Ok(snap_path);
        }
    }

    Err(SteamDetectorError::SteamNotFound)
}

/// Gets all Steam library folders by parsing libraryfolders.vdf
pub fn get_steam_library_folders(steam_path: &Path) -> Result<Vec<PathBuf>, SteamDetectorError> {
    let library_vdf_path = steam_path.join("steamapps/libraryfolders.vdf");
    
    if !library_vdf_path.exists() {
        warn!("libraryfolders.vdf not found at {:?}", library_vdf_path);
        return Ok(vec![steam_path.to_path_buf()]);
    }

    let vdf_content = fs::read_to_string(&library_vdf_path)?;
    
    let mut library_paths = vec![steam_path.to_path_buf()];
    
    // Parse the VDF file using keyvalues-parser
    match keyvalues_parser::Vdf::parse(&vdf_content) {
        Ok(vdf) => {
            if let Some(obj) = vdf.value.get_obj() {
                // Iterate over the BTreeMap
                for (_key, values) in obj.iter() {
                    // Each value in the Vec might be an Obj
                    for value in values {
                        if let Some(folder_obj) = value.get_obj() {
                            // Look for the "path" key
                            if let Some(path_values) = folder_obj.get("path") {
                                if let Some(first_value) = path_values.first() {
                                    if let Some(path_str) = first_value.get_str() {
                                        let path = PathBuf::from(path_str.to_string());
                                        if path.exists() {
                                            library_paths.push(path);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            warn!("Failed to parse libraryfolders.vdf: {:?}", e);
            // Return default path if parsing fails
            return Ok(vec![steam_path.to_path_buf()]);
        }
    }

    info!("Found {} Steam library folders", library_paths.len());
    Ok(library_paths)
}

/// Parses an appmanifest file to get game info
fn parse_appmanifest(manifest_path: &PathBuf) -> Result<(String, String, String), SteamDetectorError> {
    let content = fs::read_to_string(manifest_path)?;
    
    match keyvalues_parser::Vdf::parse(&content) {
        Ok(vdf) => {
            if let Some(obj) = vdf.value.get_obj() {
                // Get the first value in the object (AppState)
                let app_id = obj
                    .get("appid")
                    .and_then(|v| v.first())
                    .and_then(|v| v.get_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                
                let name = obj
                    .get("name")
                    .and_then(|v| v.first())
                    .and_then(|v| v.get_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "Unknown".to_string());
                
                let install_dir = obj
                    .get("installdir")
                    .and_then(|v| v.first())
                    .and_then(|v| v.get_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                Ok((app_id, name, install_dir))
            } else {
                Err(SteamDetectorError::VdfParseError(
                    "Invalid appmanifest structure".to_string(),
                ))
            }
        }
        Err(e) => Err(SteamDetectorError::VdfParseError(format!("{:?}", e))),
    }
}

/// Scans all Steam library folders for installed games
pub fn scan_steam_games(library_folders: &[PathBuf]) -> Result<Vec<SteamGame>, SteamDetectorError> {
    let mut games = Vec::new();

    for library_path in library_folders {
        let steamapps_path = library_path.join("steamapps");
        
        if !steamapps_path.exists() {
            debug!("steamapps folder not found at {:?}", steamapps_path);
            continue;
        }

        // Read all appmanifest files
        match fs::read_dir(&steamapps_path) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    
                    if let Some(file_name) = path.file_name() {
                        let file_name_str = file_name.to_string_lossy();
                        
                        if file_name_str.starts_with("appmanifest_") && file_name_str.ends_with(".acf") {
                            match parse_appmanifest(&path) {
                                Ok((app_id, name, install_dir)) => {
                                    if !install_dir.is_empty() {
                                        let install_path = steamapps_path
                                            .join("common")
                                            .join(&install_dir);
                                        
                                        if install_path.exists() {
                                            info!("Found game: {} ({})", name, app_id);
                                            games.push(SteamGame {
                                                app_id,
                                                name,
                                                install_dir,
                                                install_path,
                                                save_paths: Vec::new(),
                                            });
                                        }
                                    }
                                }
                                Err(e) => {
                                    debug!("Failed to parse {:?}: {:?}", path, e);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to read steamapps directory: {:?}", e);
            }
        }
    }

    info!("Found {} installed Steam games", games.len());
    Ok(games)
}

/// Downloads and parses the ludusavi manifest to get save locations
pub fn download_ludusavi_manifest() -> Result<HashMap<String, ManifestEntry>, SteamDetectorError> {
    info!("Downloading ludusavi manifest...");
    
    let url = "https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml";
    
    let response = reqwest::blocking::get(url).map_err(|e| {
        SteamDetectorError::ManifestDownloadError(format!("Failed to download: {}", e))
    })?;

    if !response.status().is_success() {
        return Err(SteamDetectorError::ManifestDownloadError(format!(
            "HTTP error: {}",
            response.status()
        )));
    }

    let content = response.text().map_err(|e| {
        SteamDetectorError::ManifestDownloadError(format!("Failed to read response: {}", e))
    })?;

    parse_ludusavi_manifest(&content)
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ManifestEntry {
    pub files: Option<HashMap<String, FileEntry>>,
    pub steam: Option<SteamInfo>,
    #[serde(rename = "installDir")]
    pub install_dir: Option<HashMap<String, serde_yaml::Value>>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct FileEntry {
    pub tags: Option<Vec<String>>,
    pub when: Option<Vec<HashMap<String, String>>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SteamInfo {
    pub id: u32,
}

/// Parses the ludusavi manifest YAML
fn parse_ludusavi_manifest(
    content: &str,
) -> Result<HashMap<String, ManifestEntry>, SteamDetectorError> {
    serde_yaml::from_str(content).map_err(|e| {
        SteamDetectorError::ManifestParseError(format!("Failed to parse YAML: {}", e))
    })
}

/// Matches Steam games with their save locations from the manifest
pub fn match_games_with_manifest(
    games: &mut [SteamGame],
    manifest: &HashMap<String, ManifestEntry>,
) {
    let mut steam_id_map: HashMap<u32, &ManifestEntry> = HashMap::new();
    
    // Build a map of Steam IDs to manifest entries
    for entry in manifest.values() {
        if let Some(steam_info) = &entry.steam {
            steam_id_map.insert(steam_info.id, entry);
        }
    }

    for game in games.iter_mut() {
        if let Ok(app_id) = game.app_id.parse::<u32>() {
            if let Some(manifest_entry) = steam_id_map.get(&app_id) {
                if let Some(files) = &manifest_entry.files {
                    for (path, file_entry) in files {
                        // Check if this is a save file (has "save" tag)
                        if let Some(tags) = &file_entry.tags {
                            if tags.contains(&"save".to_string()) {
                                // Determine if it's a file or directory
                                let path_type = if path.ends_with('/') {
                                    "directory"
                                } else {
                                    "file"
                                };

                                game.save_paths.push(SteamSavePath {
                                    path: path.clone(),
                                    path_type: path_type.to_string(),
                                });
                            }
                        }
                    }
                }
                
                if !game.save_paths.is_empty() {
                    info!(
                        "Matched {} save paths for game: {}",
                        game.save_paths.len(),
                        game.name
                    );
                }
            }
        }
    }
}

/// Main function to detect all Steam games with save locations
pub fn detect_steam_games_with_saves() -> Result<Vec<SteamGame>, SteamDetectorError> {
    info!("Starting Steam game detection...");
    
    let steam_path = detect_steam_path()?;
    info!("Found Steam installation at: {:?}", steam_path);
    
    let library_folders = get_steam_library_folders(&steam_path)?;
    
    let mut games = scan_steam_games(&library_folders)?;
    
    // Download and match with manifest
    match download_ludusavi_manifest() {
        Ok(manifest) => {
            info!("Successfully downloaded ludusavi manifest");
            match_games_with_manifest(&mut games, &manifest);
        }
        Err(e) => {
            error!("Failed to download ludusavi manifest: {:?}", e);
            warn!("Continuing without save path information");
        }
    }

    Ok(games)
}
