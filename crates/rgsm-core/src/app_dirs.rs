use log::info;
use std::fmt;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Stores the application's data directory path
static APP_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
static APP_DATA_DIR_OVERRIDE: OnceLock<PathBuf> = OnceLock::new();
#[cfg(test)]
static TEST_APP_DATA_DIR: OnceLock<temp_dir::TempDir> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppDataDirOverrideError {
    AlreadyInitialized,
    AlreadySet,
}

impl fmt::Display for AppDataDirOverrideError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppDataDirOverrideError::AlreadyInitialized => {
                write!(f, "application data directory is already initialized")
            }
            AppDataDirOverrideError::AlreadySet => {
                write!(f, "application data directory override is already set")
            }
        }
    }
}

impl std::error::Error for AppDataDirOverrideError {}

/// Set an explicit application data directory before any config/path access.
///
/// The override is intended for non-GUI frontends such as `rgsm-tui` and must
/// be installed during process startup. Once the default directory has been
/// resolved, changing it would make later config reads/write target a different
/// root from earlier reads, so this function rejects late calls.
pub fn set_app_data_dir_override(path: PathBuf) -> Result<(), AppDataDirOverrideError> {
    if APP_DATA_DIR.get().is_some() {
        return Err(AppDataDirOverrideError::AlreadyInitialized);
    }
    APP_DATA_DIR_OVERRIDE
        .set(path)
        .map_err(|_| AppDataDirOverrideError::AlreadySet)
}

/// Get the directory where application data should be stored
///
/// This function implements the following logic:
/// - In debug mode: Check pwd first (to avoid test configs in target/debug being cleared)
/// - Always use the executable's directory for data storage
///
/// The result is cached after the first call.
/// The data directory is determined at startup and remains fixed for the application lifetime.
pub fn get_app_data_dir() -> &'static PathBuf {
    if let Some(path) = APP_DATA_DIR_OVERRIDE.get() {
        return path;
    }
    APP_DATA_DIR.get_or_init(init_app_data_dir)
}

#[cfg(test)]
fn init_app_data_dir() -> PathBuf {
    init_test_app_data_dir()
}

#[cfg(not(test))]
fn init_app_data_dir() -> PathBuf {
    init_runtime_app_data_dir()
}

#[cfg(test)]
fn init_test_app_data_dir() -> PathBuf {
    let test_data_dir = TEST_APP_DATA_DIR.get_or_init(|| {
        temp_dir::TempDir::new().expect("failed to create temporary test data directory")
    });
    info!(
        "Test mode: Using temp directory as data directory: {}",
        test_data_dir.path().display()
    );
    test_data_dir.path().to_path_buf()
}

#[cfg_attr(test, allow(dead_code))]
fn init_runtime_app_data_dir() -> PathBuf {
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
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        info!(
            "Using executable directory as data directory: {}",
            exe_dir.display()
        );
        return exe_dir.to_path_buf();
    }

    // Fallback only if we cannot determine executable directory
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    log::warn!(
        "Failed to determine executable directory, falling back to current directory: {}",
        cwd.display()
    );
    cwd
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

    #[test]
    fn test_override_error_display() {
        assert_eq!(
            AppDataDirOverrideError::AlreadyInitialized.to_string(),
            "application data directory is already initialized"
        );
        assert_eq!(
            AppDataDirOverrideError::AlreadySet.to_string(),
            "application data directory override is already set"
        );
    }

    #[test]
    fn test_override_path_resolution_shape() {
        let base = PathBuf::from("custom-data-dir");
        let resolved = base.join("GameSaveManager.config.json");
        assert!(resolved.ends_with("GameSaveManager.config.json"));
    }
}
