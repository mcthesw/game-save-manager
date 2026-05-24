use std::path::PathBuf;

use anyhow::{Result, bail};

pub const DATA_DIR_ENV: &str = "RGSM_TUI_DATA_DIR";

#[derive(Debug, Clone, Default)]
pub struct CliOptions {
    pub data_dir: Option<PathBuf>,
    pub import_gui_config: Option<PathBuf>,
    pub help: bool,
}

impl CliOptions {
    pub fn parse() -> Result<Self> {
        let mut options = Self::parse_from(std::env::args().skip(1))?;
        if options.data_dir.is_none()
            && let Ok(value) = std::env::var(DATA_DIR_ENV)
            && !value.trim().is_empty()
        {
            options.data_dir = Some(PathBuf::from(value));
        }
        Ok(options)
    }

    pub fn parse_from<I, S>(args: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut options = CliOptions::default();
        let mut args = args.into_iter().map(Into::into);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-h" | "--help" => options.help = true,
                "--data-dir" => {
                    let Some(value) = args.next() else {
                        bail!("--data-dir requires a path");
                    };
                    options.data_dir = Some(PathBuf::from(value));
                }
                _ if arg.starts_with("--data-dir=") => {
                    options.data_dir = Some(PathBuf::from(&arg["--data-dir=".len()..]));
                }
                "--import-gui-config" => {
                    let Some(value) = args.next() else {
                        bail!("--import-gui-config requires a path");
                    };
                    options.import_gui_config = Some(PathBuf::from(value));
                }
                _ if arg.starts_with("--import-gui-config=") => {
                    options.import_gui_config =
                        Some(PathBuf::from(&arg["--import-gui-config=".len()..]));
                }
                _ => bail!("unknown argument: {arg}"),
            }
        }

        Ok(options)
    }

    pub fn resolved_data_dir(&self) -> PathBuf {
        self.data_dir.clone().unwrap_or_else(default_tui_data_dir)
    }
}

pub fn help_text() -> String {
    format!(
        "rgsm-tui\n\nUSAGE:\n  cargo run -p rgsm-tui -- [--data-dir <path>] [--import-gui-config <path>]\n\nOPTIONS:\n  --data-dir <path>           Use an explicit TUI profile directory\n  --import-gui-config <path>  Import a GUI profile directory or GameSaveManager.config.json into the TUI profile\n  -h, --help                  Show this help\n\nENV:\n  {DATA_DIR_ENV}=<path>  TUI profile directory used when --data-dir is omitted\n"
    )
}

fn default_tui_data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(base) = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("APPDATA"))
        {
            return PathBuf::from(base).join("GameSaveManager").join("tui");
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("GameSaveManager")
                .join("tui");
        }
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        if let Some(base) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(base).join("game-save-manager").join("tui");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("game-save-manager")
                .join("tui");
        }
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".rgsm-tui")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_data_dir_flag() {
        let parsed = CliOptions::parse_from(["--data-dir", "tmp/rgsm"]).unwrap();
        assert_eq!(parsed.data_dir, Some(PathBuf::from("tmp/rgsm")));
    }

    #[test]
    fn parses_import_gui_config_flag() {
        let parsed = CliOptions::parse_from(["--import-gui-config", "tmp/gui"]).unwrap();
        assert_eq!(parsed.import_gui_config, Some(PathBuf::from("tmp/gui")));
    }

    #[test]
    fn rejects_missing_data_dir_value() {
        assert!(CliOptions::parse_from(["--data-dir"]).is_err());
    }

    #[test]
    fn rejects_missing_import_gui_config_value() {
        assert!(CliOptions::parse_from(["--import-gui-config"]).is_err());
    }
}
