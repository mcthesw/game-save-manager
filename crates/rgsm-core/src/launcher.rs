use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
enum LaunchStrategy {
    OpenWithSystem,
    RunExecutable { working_dir: PathBuf },
}

pub fn open_path(path: &Path) -> Result<()> {
    match launch_strategy(path) {
        LaunchStrategy::OpenWithSystem => {
            open::that(path).with_context(|| format!("Failed to open path '{}'", path.display()))?
        }
        LaunchStrategy::RunExecutable { working_dir } => {
            Command::new(path)
                .current_dir(&working_dir)
                .spawn()
                .with_context(|| {
                    format!(
                        "Failed to launch executable '{}' with working directory '{}'",
                        path.display(),
                        working_dir.display()
                    )
                })?;
        }
    }

    Ok(())
}

fn launch_strategy(path: &Path) -> LaunchStrategy {
    if should_launch_executable_in_place(path) {
        let working_dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        LaunchStrategy::RunExecutable { working_dir }
    } else {
        LaunchStrategy::OpenWithSystem
    }
}

fn should_launch_executable_in_place(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("exe"))
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{LaunchStrategy, launch_strategy};
    use std::fs;
    use temp_dir::TempDir;

    #[test]
    fn uses_system_open_for_directories() {
        let temp_dir = TempDir::new().unwrap();
        let game_dir = temp_dir.path().join("game");
        fs::create_dir(&game_dir).unwrap();

        assert_eq!(launch_strategy(&game_dir), LaunchStrategy::OpenWithSystem);
    }

    #[test]
    fn launches_exe_from_its_parent_directory() {
        let temp_dir = TempDir::new().unwrap();
        let game_dir = temp_dir.path().join("game");
        fs::create_dir(&game_dir).unwrap();
        let exe_path = game_dir.join("Turing Complete.EXE");
        fs::write(&exe_path, []).unwrap();

        assert_eq!(
            launch_strategy(&exe_path),
            LaunchStrategy::RunExecutable {
                working_dir: game_dir,
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
}
