use std::path::PathBuf;
use std::sync::OnceLock;
use log::info;

/// Stores the application's data directory path
static APP_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Determines if the application is running in portable mode
///
/// Portable mode is detected when:
/// 1. A config file exists next to the executable, OR
/// 2. The executable is NOT in a typical system installation directory
///    (Program Files, /usr/, /opt/, /Applications/, AppData/Local, etc.)
///    This heuristic assumes executables outside these directories are portable,
///    including those in user folders like Downloads, Desktop, or the home directory.
fn is_portable_mode() -> bool {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Check if config exists next to executable (explicit portable mode)
            let config_path = exe_dir.join("GameSaveManager.config.json");
            if config_path.exists() {
                return true;
            }
            
            // Check if we're in a typical installation directory
            // If not, assume portable mode for new installations
            let exe_dir_str = exe_dir.to_string_lossy().to_lowercase();
            let is_installed = exe_dir_str.contains("program files")
                || exe_dir_str.contains("programme") // German/French for Program Files
                || exe_dir_str.contains("/usr/")
                || exe_dir_str.contains("/opt/")
                || exe_dir_str.contains("/applications/")
                || exe_dir_str.contains("appdata\\local")
                || exe_dir_str.contains("appdata/local");
            
            return !is_installed;
        }
    }
    false
}

/// Get the directory where application data should be stored
///
/// This function implements the following logic:
/// - In portable mode: use the executable's directory
/// - Otherwise: use the current working directory for backwards compatibility
///
/// The result is cached after the first call.
pub fn get_app_data_dir() -> &'static PathBuf {
    APP_DATA_DIR.get_or_init(|| {
        if is_portable_mode() {
            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(exe_dir) = exe_path.parent() {
                    info!("Using portable mode: data directory at {}", exe_dir.display());
                    return exe_dir.to_path_buf();
                }
            }
        }
        
        // Fall back to current working directory for backwards compatibility
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        info!("Using current working directory: {}", cwd.display());
        cwd
    })
}

/// Resolve a path relative to the app data directory
///
/// If the path is already absolute, return it as-is.
/// Otherwise, resolve it relative to the app data directory.
pub fn resolve_app_path(path: &str) -> PathBuf {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        return candidate;
    }
    
    get_app_data_dir().join(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_absolute_path() {
        #[cfg(target_os = "windows")]
        let absolute_path = "C:\\test\\path";
        #[cfg(not(target_os = "windows"))]
        let absolute_path = "/test/path";

        let result = resolve_app_path(absolute_path);
        assert_eq!(result, PathBuf::from(absolute_path));
    }

    #[test]
    fn test_resolve_relative_path() {
        let relative_path = "config.json";
        let result = resolve_app_path(relative_path);
        
        // The result should be relative to app data dir
        assert!(result.ends_with(relative_path));
    }
}
