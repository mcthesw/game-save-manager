use std::fs;
use std::path::{Path, PathBuf};

const PATH_VARIABLES: &[&str] = &[
    "<home>",
    "<osUserName>",
    "<winAppData>",
    "<winLocalAppData>",
    "<winLocalAppDataLow>",
    "<winDocuments>",
    "<winPublic>",
    "<winProgramData>",
    "<winDir>",
    "<xdgData>",
    "<xdgConfig>",
    "<root>",
    "<base>",
    "<game>",
    "<storeUserId>",
    "<storeGameId>",
];

pub fn complete_path(input: &str) -> Vec<String> {
    if input.starts_with('<') {
        return PATH_VARIABLES
            .iter()
            .copied()
            .filter(|value| value.starts_with(input))
            .map(str::to_string)
            .collect();
    }

    let candidate = PathBuf::from(input);
    let (parent, prefix) = if input.ends_with(std::path::MAIN_SEPARATOR) {
        (candidate.as_path(), "")
    } else {
        (
            candidate.parent().unwrap_or_else(|| Path::new(".")),
            candidate
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(""),
        )
    };

    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };

    let mut matches = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(prefix) {
                return None;
            }
            Some(entry.path().to_string_lossy().to_string())
        })
        .collect::<Vec<_>>();
    matches.sort();
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completes_rgsm_variables() {
        let result = complete_path("<sto");
        assert!(result.contains(&"<storeUserId>".to_string()));
        assert!(result.contains(&"<storeGameId>".to_string()));
    }

    #[test]
    fn completes_filesystem_paths() {
        let temp = temp_dir::TempDir::new().unwrap();
        fs::write(temp.path().join("alpha.sav"), b"save").unwrap();

        let input = temp.path().join("a").to_string_lossy().to_string();
        let result = complete_path(&input);

        assert_eq!(result.len(), 1);
        assert!(result[0].ends_with("alpha.sav"));
    }
}
