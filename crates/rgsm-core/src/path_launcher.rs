use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Config;
use crate::path_resolver::{self, PathContext};

#[derive(Debug, Clone, PartialEq, Eq)]
enum LaunchStrategy {
    OpenWithSystem,
    RunDirectly { working_dir: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ManagedLaunchTarget {
    Filesystem(PathBuf),
    Registry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenManagedLocationOutcome {
    Opened,
    Warning(OpenManagedLocationWarning),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenManagedLocationWarning {
    RegistryOpenUnsupported,
}

pub fn open_path(path: &Path) -> Result<()> {
    match launch_strategy(path) {
        LaunchStrategy::OpenWithSystem => {
            open::that(path).with_context(|| format!("Failed to open path '{}'", path.display()))?
        }
        LaunchStrategy::RunDirectly { working_dir } => {
            Command::new(path)
                .current_dir(&working_dir)
                .spawn()
                .with_context(|| {
                    format!(
                        "Failed to launch '{}' with working directory '{}'",
                        path.display(),
                        working_dir.display()
                    )
                })?;
        }
    }

    Ok(())
}

pub fn open_managed_location(
    raw_path: &str,
    path_ctx: Option<&PathContext>,
    config: &Config,
) -> Result<OpenManagedLocationOutcome> {
    match managed_launch_target(raw_path, path_ctx, config)? {
        ManagedLaunchTarget::Filesystem(path) => {
            open_path(&path)?;
            Ok(OpenManagedLocationOutcome::Opened)
        }
        ManagedLaunchTarget::Registry => Ok(OpenManagedLocationOutcome::Warning(
            OpenManagedLocationWarning::RegistryOpenUnsupported,
        )),
    }
}

fn launch_strategy(path: &Path) -> LaunchStrategy {
    if should_run_directly(path) {
        let working_dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        LaunchStrategy::RunDirectly { working_dir }
    } else {
        LaunchStrategy::OpenWithSystem
    }
}

fn managed_launch_target(
    raw_path: &str,
    path_ctx: Option<&PathContext>,
    config: &Config,
) -> Result<ManagedLaunchTarget> {
    if crate::backup::registry::is_registry_path(raw_path) {
        return Ok(ManagedLaunchTarget::Registry);
    }

    let path = path_resolver::resolve_path(raw_path, path_ctx, config)
        .with_context(|| format!("Failed to resolve path '{raw_path}'"))?;
    Ok(ManagedLaunchTarget::Filesystem(path))
}

#[cfg(target_os = "windows")]
fn should_run_directly(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "exe" | "com"))
            .unwrap_or(false)
}

#[cfg(unix)]
fn should_run_directly(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(any(target_os = "windows", unix)))]
fn should_run_directly(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::{
        LaunchStrategy, ManagedLaunchTarget, OpenManagedLocationOutcome,
        OpenManagedLocationWarning, launch_strategy, managed_launch_target, open_managed_location,
    };
    use crate::config::Config;
    use std::fs;
    use temp_dir::TempDir;

    #[test]
    fn uses_system_open_for_directories() {
        let temp_dir = TempDir::new().unwrap();
        let game_dir = temp_dir.path().join("game");
        fs::create_dir(&game_dir).unwrap();

        assert_eq!(launch_strategy(&game_dir), LaunchStrategy::OpenWithSystem);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn launches_windows_executables_from_their_parent_directory() {
        let temp_dir = TempDir::new().unwrap();
        let game_dir = temp_dir.path().join("game");
        fs::create_dir(&game_dir).unwrap();
        let exe_path = game_dir.join("Turing Complete.EXE");
        fs::write(&exe_path, []).unwrap();

        assert_eq!(
            launch_strategy(&exe_path),
            LaunchStrategy::RunDirectly {
                working_dir: game_dir
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn launches_unix_executables_from_their_parent_directory() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let game_dir = temp_dir.path().join("game");
        fs::create_dir(&game_dir).unwrap();
        let binary_path = game_dir.join("game.sh");
        fs::write(&binary_path, []).unwrap();

        let mut permissions = fs::metadata(&binary_path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&binary_path, permissions).unwrap();

        assert_eq!(
            launch_strategy(&binary_path),
            LaunchStrategy::RunDirectly {
                working_dir: game_dir
            }
        );
    }

    #[test]
    fn keeps_non_executable_files_on_system_open_path() {
        let temp_dir = TempDir::new().unwrap();
        let shortcut_path = temp_dir.path().join("game.lnk");
        fs::write(&shortcut_path, []).unwrap();

        assert_eq!(
            launch_strategy(&shortcut_path),
            LaunchStrategy::OpenWithSystem
        );
    }

    #[test]
    fn routes_registry_locations_without_filesystem_resolution() {
        let target = managed_launch_target(
            "REGISTRY:HKEY_CURRENT_USER/Software/RGSM Test",
            None,
            &Config::default(),
        )
        .unwrap();

        assert_eq!(target, ManagedLaunchTarget::Registry);
    }

    #[test]
    fn warns_instead_of_launching_registry_locations() {
        let outcome = open_managed_location(
            "REGISTRY:HKEY_CURRENT_USER/Software/RGSM Test",
            None,
            &Config::default(),
        )
        .unwrap();

        assert_eq!(
            outcome,
            OpenManagedLocationOutcome::Warning(
                OpenManagedLocationWarning::RegistryOpenUnsupported
            )
        );
    }

    #[test]
    fn routes_filesystem_locations_through_path_resolution() {
        let target = managed_launch_target("<home>", None, &Config::default()).unwrap();

        assert!(matches!(target, ManagedLaunchTarget::Filesystem(_)));
    }
}
