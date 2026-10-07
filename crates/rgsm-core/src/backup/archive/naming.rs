use std::path::Path;

use serde::{Deserialize, Deserializer};

use crate::{backup::Snapshot, preclude::BackupError};

pub(crate) fn deserialize_archive_name<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    let name = Option::<String>::deserialize(deserializer)?;
    if name.as_ref().is_some_and(|name| !valid_archive_name(name)) {
        return Err(serde::de::Error::custom("invalid archive basename"));
    }
    Ok(name)
}

fn valid_archive_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        && (name.ends_with(".7z") || name.ends_with(".zip"))
}

/// Preserve an original historical clock without inventing its time zone.
pub(crate) fn new_archive_name(
    snapshot: &Snapshot,
    directory: &Path,
) -> Result<String, BackupError> {
    let time = snapshot
        .created_at
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y-%m-%d_%H-%M-%S")
                .to_string()
        })
        .or_else(|| legacy_archive_time(&snapshot.date))
        .unwrap_or_else(|| "unknown-time".into());
    for _ in 0..3 {
        let random = uuid::Uuid::new_v4().simple().to_string();
        let name = format!("{time}_{}.7z", &random[..12]);
        if !directory.join(&name).exists() {
            return Ok(name);
        }
    }
    Err(BackupError::Unexpected(anyhow::anyhow!(
        "could not allocate an archive filename"
    )))
}

pub(super) fn legacy_archive_time(id: &str) -> Option<String> {
    let clock = id.strip_prefix("Overwrite_").unwrap_or(id);
    chrono::NaiveDateTime::parse_from_str(clock, "%Y-%m-%d_%H-%M-%S")
        .ok()
        .map(|time| time.format("%Y-%m-%d_%H-%M-%S").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialized_names_cannot_escape_the_game_directory() {
        for name in [
            "../other.7z",
            "C:/outside.7z",
            "sub\\archive.7z",
            "bad:stream.7z",
            "bad\n.7z",
        ] {
            let result = serde_json::from_value::<Snapshot>(serde_json::json!({
                "date": "id", "describe": "", "path": "", "archive_name": name
            }));
            assert!(result.is_err(), "{name}");
        }
    }

    #[test]
    fn historical_wall_time_is_preserved_and_unknown_time_is_not_invented() {
        let temp = temp_dir::TempDir::new().unwrap();
        for (id, prefix) in [
            ("2020-05-06_07-08-09", "2020-05-06_07-08-09_"),
            ("opaque", "unknown-time_"),
        ] {
            let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
                "date": id, "describe": "", "path": ""
            }))
            .unwrap();
            let first = new_archive_name(&snapshot, temp.path()).unwrap();
            let second = new_archive_name(&snapshot, temp.path()).unwrap();
            assert!(first.starts_with(prefix));
            assert_ne!(first, second);
            assert!(snapshot.created_at.is_none());
        }
    }
}
