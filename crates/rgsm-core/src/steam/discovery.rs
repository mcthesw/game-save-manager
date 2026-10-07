use std::{env, path::PathBuf};

/// Try to get Steam installation path from Windows Registry
#[cfg(target_os = "windows")]
fn get_steam_path_from_registry() -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::*;

    // Try HKEY_CURRENT_USER first (user-specific installation)
    if let Ok(hkcu) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam")
        && let Ok(path) = hkcu.get_value::<String, _>("SteamPath")
    {
        let normalized = path.replace('/', "\\");
        if std::path::Path::new(&normalized).exists() {
            return Some(normalized);
        }
    }

    // Try HKEY_LOCAL_MACHINE (machine-wide installation, 32-bit on 64-bit Windows)
    if let Ok(hklm) =
        RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("SOFTWARE\\WOW6432Node\\Valve\\Steam")
        && let Ok(path) = hklm.get_value::<String, _>("InstallPath")
    {
        let normalized = path.replace('/', "\\");
        if std::path::Path::new(&normalized).exists() {
            return Some(normalized);
        }
    }

    // Try HKEY_LOCAL_MACHINE (32-bit Windows or native key)
    if let Ok(hklm) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("SOFTWARE\\Valve\\Steam")
        && let Ok(path) = hklm.get_value::<String, _>("InstallPath")
    {
        let normalized = path.replace('/', "\\");
        if std::path::Path::new(&normalized).exists() {
            return Some(normalized);
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
pub(super) fn steam_root() -> Result<PathBuf, super::SteamError> {
    let mut steam_roots: Vec<String> = Vec::new();

    // First, try environment variable
    if let Ok(env_root) = env::var("STEAM_DIR")
        && !env_root.trim().is_empty()
    {
        steam_roots.push(env_root);
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
        .map(PathBuf::from)
        .ok_or(super::SteamError::SteamNotFound)
}
