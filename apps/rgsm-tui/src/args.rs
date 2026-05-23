use std::path::PathBuf;

use anyhow::{Result, bail};

pub const DATA_DIR_ENV: &str = "RGSM_DATA_DIR";

#[derive(Debug, Clone, Default)]
pub struct CliOptions {
    pub data_dir: Option<PathBuf>,
    pub help: bool,
}

impl CliOptions {
    pub fn parse() -> Result<Self> {
        Self::parse_from(std::env::args().skip(1))
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
                _ => bail!("unknown argument: {arg}"),
            }
        }

        if options.data_dir.is_none()
            && let Ok(value) = std::env::var(DATA_DIR_ENV)
            && !value.trim().is_empty()
        {
            options.data_dir = Some(PathBuf::from(value));
        }

        Ok(options)
    }
}

pub fn help_text() -> String {
    format!(
        "rgsm-tui\n\nUSAGE:\n  cargo run -p rgsm-tui -- [--data-dir <path>]\n\nOPTIONS:\n  --data-dir <path>  Use an explicit RGSM data directory\n  -h, --help         Show this help\n\nENV:\n  {DATA_DIR_ENV}=<path>  Data directory override used when --data-dir is omitted\n"
    )
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
    fn rejects_missing_data_dir_value() {
        assert!(CliOptions::parse_from(["--data-dir"]).is_err());
    }
}
