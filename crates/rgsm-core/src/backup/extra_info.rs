use std::path::PathBuf;

use crate::backup::Game;
use crate::config::get_backup_path;
use crate::preclude::BackupError;

pub const EXTRA_INFO_DIR: &str = "extra_info";

pub fn extra_info_dir(game: &Game) -> Result<PathBuf, BackupError> {
    Ok(get_backup_path()?
        .join(game.backup_dir_name().as_ref())
        .join(EXTRA_INFO_DIR))
}

pub fn extra_info_namespace_dir(game: &Game, namespace: &str) -> Result<PathBuf, BackupError> {
    validate_extra_info_namespace(namespace)?;
    Ok(extra_info_dir(game)?.join(namespace))
}

pub fn extra_info_namespace_file(
    game: &Game,
    namespace: &str,
    file_name: &str,
) -> Result<PathBuf, BackupError> {
    validate_extra_info_file_name(file_name)?;
    Ok(extra_info_namespace_dir(game, namespace)?.join(file_name))
}

fn validate_extra_info_namespace(namespace: &str) -> Result<(), BackupError> {
    let valid = !namespace.is_empty()
        && namespace
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'));
    if valid {
        Ok(())
    } else {
        Err(BackupError::Unexpected(anyhow::anyhow!(
            "invalid extra_info namespace '{namespace}'"
        )))
    }
}

fn validate_extra_info_file_name(file_name: &str) -> Result<(), BackupError> {
    let valid = !file_name.is_empty()
        && !file_name.contains('/')
        && !file_name.contains('\\')
        && file_name != "."
        && file_name != "..";
    if valid {
        Ok(())
    } else {
        Err(BackupError::Unexpected(anyhow::anyhow!(
            "invalid extra_info file name '{file_name}'"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_extra_info_namespaces() {
        for namespace in ["metadata", "plugin.example", "plugin-example_1"] {
            assert!(validate_extra_info_namespace(namespace).is_ok());
        }

        for namespace in ["", "../plugin", "plugin/data", "plugin data"] {
            assert!(validate_extra_info_namespace(namespace).is_err());
        }
    }

    #[test]
    fn validates_extra_info_file_names() {
        for file_name in ["manifest.json", "2026-05-29.json", "media_1.webp"] {
            assert!(validate_extra_info_file_name(file_name).is_ok());
        }

        for file_name in ["", ".", "..", "../manifest.json", "metadata\\1.json"] {
            assert!(validate_extra_info_file_name(file_name).is_err());
        }
    }
}
