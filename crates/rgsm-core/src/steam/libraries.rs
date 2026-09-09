//! Steam library discovery and lexical path deduplication.

use super::{LibraryFolder, SteamError, get_steam_root};
use log::{debug, warn};
use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
};

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

    let library_folders: BTreeMap<String, LibraryFolder> = keyvalues_serde::from_str(&content)
        .map_err(|e| SteamError::VdfParse {
            path: vdf_path.clone(),
            reason: e.to_string(),
        })?;

    let mut libraries = vec![steam_root.clone()];
    for library in library_folders.into_values().map(|folder| {
        let p = folder.path.replace('\\', "/");
        PathBuf::from(p)
    }) {
        if library.exists() {
            libraries.push(library);
        }
    }
    let libraries = unique_library_paths(libraries);

    debug!(target: "rgsm::steam", "Found {} Steam libraries", libraries.len());
    Ok(libraries)
}

fn unique_library_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for path in paths {
        let raw = path.to_string_lossy();
        let windows_path = is_windows_library_path(&raw);
        let display = if windows_path {
            raw.replace('\\', "/")
        } else {
            raw.into_owned()
        };
        let key = if windows_path {
            display.to_lowercase()
        } else {
            display.clone()
        };
        if seen.insert(key.trim_end_matches('/').to_string()) {
            result.push(PathBuf::from(display));
        }
    }
    result
}

fn is_windows_library_path(path: &str) -> bool {
    path.starts_with(r"\\")
        || path.starts_with("//")
        || (path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
            && path.as_bytes().get(1) == Some(&b':')
            && matches!(path.as_bytes().get(2), Some(b'/' | b'\\')))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_paths_use_windows_identity_but_preserve_unix_case() {
        let paths = [
            r"c:\program files (x86)\steam",
            "C:/Program Files (x86)/Steam/",
            "D:/Games",
            r"d:\games\",
            "/games",
            "/Games",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
        assert_eq!(unique_library_paths(paths).len(), 4);
    }
}
