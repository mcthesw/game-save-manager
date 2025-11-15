use log::info;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Stores the application's data directory path
static APP_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Get the directory where application data should be stored
///
/// This function implements the following logic:
/// - In debug mode: Check pwd first (to avoid test configs in target/debug being cleared)
/// - Always use the executable's directory for data storage
///
/// The result is cached after the first call.
/// The data directory is determined at startup and remains fixed for the application lifetime.
pub fn get_app_data_dir() -> &'static PathBuf {
    APP_DATA_DIR.get_or_init(|| {
        // In debug mode, check pwd first to avoid test configs in target/debug
        // being cleared during cargo clean or rebuilds
        #[cfg(debug_assertions)]
        {
            if let Ok(cwd) = std::env::current_dir() {
                let pwd_config_path = cwd.join("GameSaveManager.config.json");
                if pwd_config_path.exists() {
                    info!("Debug mode: Using pwd as data directory: {}", cwd.display());
                    return cwd;
                }
            }
        }

        // Standard behavior: use executable directory for both portable and installed versions
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                info!(
                    "Using executable directory as data directory: {}",
                    exe_dir.display()
                );
                return exe_dir.to_path_buf();
            }
        }

        // Fallback only if we cannot determine executable directory
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        log::warn!(
            "Failed to determine executable directory, falling back to current directory: {}",
            cwd.display()
        );
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
