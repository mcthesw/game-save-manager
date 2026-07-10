use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use log::{info, warn};

use crate::backup::{GameDraft, SaveUnitDraft, SaveUnitType};
use crate::device::get_current_device_id;

const COMMON_SAVE_DIRS: &[&str] = &["savedata", "SaveData", "save", "Save"];
const GENERIC_EXECUTABLE_NAMES: &[&str] = &[
    "game",
    "start",
    "play",
    "setup",
    "config",
    "uninstall",
    "uninst",
    "bgi",
    "siglusengine",
    "nekomikopack",
    "advhd",
    "krkr",
    "boot",
    "launcher",
    "engine",
    "system",
];

#[cfg(windows)]
fn is_offline(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x1000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x400000;

    let attrs = metadata.file_attributes();
    (attrs & FILE_ATTRIBUTE_OFFLINE) != 0 || (attrs & FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS) != 0
}

#[cfg(not(windows))]
fn is_offline(_metadata: &std::fs::Metadata) -> bool {
    false
}

/// Scan provided directories (1-level deep) for supported visual novels
/// and return them as `GameDraft`s.
pub fn scan_games(scan_roots: &[String]) -> Vec<GameDraft> {
    let mut results = Vec::new();
    let current_device_id = get_current_device_id().to_string();

    for root in scan_roots {
        let root_path = Path::new(root);
        if !root_path.is_dir() {
            warn!(
                target: "rgsm::vn_scanner",
                "Skipping non-directory scan root: {}",
                root
            );
            continue;
        }

        let entries = match visible_child_paths(root_path) {
            Ok(entries) => entries,
            Err(err) => {
                warn!(
                    target: "rgsm::vn_scanner",
                    "Failed to read VN scan root {}: {}",
                    root,
                    err
                );
                continue;
            }
        };

        for candidate in entries {
            if !candidate.is_dir() {
                continue;
            }

            if let Some(game_draft) = detect_vn(&candidate, &current_device_id) {
                info!(
                    target: "rgsm::vn_scanner",
                    "Detected VN candidate: {}",
                    game_draft.name
                );
                results.push(game_draft);
            }
        }
    }

    results
}

fn visible_child_paths(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    let entries = fs::read_dir(path)?
        .filter_map(Result::ok)
        .filter_map(|entry| match entry.metadata() {
            Ok(metadata) if !is_offline(&metadata) => Some(entry.path()),
            Ok(_) => None,
            Err(_) => None,
        })
        .collect();

    Ok(entries)
}

fn directory_contains_extension(path: &Path, extensions: &[&str]) -> bool {
    visible_child_paths(path)
        .ok()
        .into_iter()
        .flatten()
        .filter(|entry| entry.is_file())
        .filter_map(|entry| {
            entry
                .extension()
                .map(|ext| ext.to_string_lossy().to_lowercase())
        })
        .any(|ext| extensions.iter().any(|expected| ext == *expected))
}

fn directory_contains_file(path: &Path, file_names: &[&str]) -> bool {
    visible_child_paths(path)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            entry
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .any(|name| {
            file_names
                .iter()
                .any(|expected| name.eq_ignore_ascii_case(expected))
        })
}

fn first_existing_save_dir(path: &Path) -> Option<&'static str> {
    COMMON_SAVE_DIRS
        .iter()
        .copied()
        .find(|save_dir| path.join(save_dir).is_dir())
}

fn detect_vn(path: &Path, device_id: &str) -> Option<GameDraft> {
    detect_kirikiri(path, device_id)
        .or_else(|| detect_renpy(path, device_id))
        .or_else(|| detect_rpg_maker_mv_mz(path, device_id))
        .or_else(|| detect_rpg_maker_vx_like(path, device_id))
        .or_else(|| detect_wolf_rpg(path, device_id))
        .or_else(|| detect_generic_savedata(path, device_id))
}

fn collect_candidate_names(path: &Path) -> Vec<String> {
    let mut names = Vec::new();

    if let Some(dir_name) = path.file_name() {
        names.push(dir_name.to_string_lossy().to_string());
    }

    if let Ok(content) = fs::read_to_string(path.join("package.json")) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(name) = json.get("name").and_then(|name| name.as_str()) {
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    names.push(trimmed.to_string());
                }
            }
        }
    }

    if let Ok(content) = fs::read_to_string(path.join("Game.ini")) {
        for line in content.lines() {
            if let Some((_, title)) = line.split_once('=') {
                if line.starts_with("Title=") {
                    let title = title.trim();
                    if !title.is_empty() {
                        names.push(title.to_string());
                    }
                }
            }
        }
    }

    if let Ok(entries) = visible_child_paths(path) {
        for entry in entries {
            if !entry.is_file() {
                continue;
            }

            if let Some(ext) = entry.extension() {
                if ext.to_string_lossy().eq_ignore_ascii_case("exe") {
                    if let Some(stem) = entry.file_stem() {
                        let stem_str = stem.to_string_lossy().to_string();
                        let stem_lower = stem_str.to_lowercase();
                        let is_generic = GENERIC_EXECUTABLE_NAMES
                            .iter()
                            .any(|generic| stem_lower.starts_with(generic));
                        if !is_generic && stem_str.len() > 2 {
                            names.push(stem_str);
                        }
                    }
                }
            }
        }
    }

    let mut unique = Vec::new();
    for name in names {
        if !unique.contains(&name) {
            unique.push(name);
        }
    }
    unique
}

fn check_target(base: &Path, name: &str) -> Option<PathBuf> {
    let direct = base.join(name);
    if !direct.is_dir() {
        return None;
    }

    if let Some(save_dir) = first_existing_save_dir(&direct) {
        return Some(direct.join(save_dir));
    }

    Some(direct)
}

fn search_in_base(base: Option<PathBuf>, names: &[String], depth: u8) -> Option<PathBuf> {
    let base = base?;
    if !base.is_dir() {
        return None;
    }

    for name in names {
        if let Some(path) = check_target(&base, name) {
            return Some(path);
        }
    }

    if depth == 0 {
        return None;
    }

    if let Ok(entries) = visible_child_paths(&base) {
        for sub_path in entries {
            if !sub_path.is_dir() {
                continue;
            }

            for name in names {
                if let Some(path) = check_target(&sub_path, name) {
                    return Some(path);
                }
            }
        }
    }

    None
}

fn find_external_save_path(names: &[String]) -> Option<PathBuf> {
    if let Some(path) = search_in_base(dirs::config_dir(), names, 1) {
        return Some(path);
    }

    if let Some(path) = search_in_base(dirs::data_local_dir(), names, 1) {
        return Some(path);
    }

    if let Some(path) = search_in_base(dirs::document_dir(), names, 1) {
        return Some(path);
    }

    if let Some(home) = dirs::home_dir() {
        if let Some(path) = search_in_base(Some(home.join("Saved Games")), names, 1) {
            return Some(path);
        }
    }

    None
}

fn create_vn_draft(path: &Path, device_id: &str, save_subpath: &str) -> Option<GameDraft> {
    let name = path.file_name()?.to_string_lossy().to_string();
    let candidates = collect_candidate_names(path);

    let save_path = if let Some(external_path) = find_external_save_path(&candidates) {
        external_path
    } else if save_subpath.is_empty() {
        path.to_path_buf()
    } else {
        path.join(save_subpath)
    };

    let mut paths = HashMap::new();
    paths.insert(
        device_id.to_string(),
        save_path.to_string_lossy().to_string(),
    );

    let mut game_paths = HashMap::new();
    game_paths.insert(device_id.to_string(), path.to_string_lossy().to_string());

    let save_unit = SaveUnitDraft::concrete(None, SaveUnitType::Folder, paths, false, true);

    Some(GameDraft {
        name,
        save_paths: vec![save_unit],
        game_paths,
        ludusavi_meta: None,
        device_bindings: HashMap::new(),
    })
}

fn detect_kirikiri(path: &Path, device_id: &str) -> Option<GameDraft> {
    if !directory_contains_extension(path, &["xp3"]) {
        return None;
    }

    create_vn_draft(path, device_id, "savedata")
}

fn detect_renpy(path: &Path, device_id: &str) -> Option<GameDraft> {
    let game_dir = path.join("game");
    if !game_dir.is_dir() || !directory_contains_extension(&game_dir, &["rpa"]) {
        return None;
    }

    create_vn_draft(path, device_id, "game/saves")
}

fn detect_rpg_maker_mv_mz(path: &Path, device_id: &str) -> Option<GameDraft> {
    let has_www = path.join("www").is_dir();
    let has_package = path.join("package.json").is_file();

    if !has_www && !has_package {
        return None;
    }

    if has_www {
        create_vn_draft(path, device_id, "www/save")
    } else {
        create_vn_draft(path, device_id, "save")
    }
}

fn detect_rpg_maker_vx_like(path: &Path, device_id: &str) -> Option<GameDraft> {
    if !directory_contains_extension(path, &["rgss3a", "rgss2a", "rvdata2", "rvdata"]) {
        return None;
    }

    create_vn_draft(path, device_id, "")
}

fn detect_wolf_rpg(path: &Path, device_id: &str) -> Option<GameDraft> {
    let has_data = path.join("Data").is_dir();
    let has_game_files = directory_contains_file(path, &["Game.exe", "Game.dat"]);
    if !has_data || !has_game_files {
        return None;
    }

    create_vn_draft(path, device_id, "Save")
}

fn detect_generic_savedata(path: &Path, device_id: &str) -> Option<GameDraft> {
    if !directory_contains_extension(path, &["exe"]) {
        return None;
    }

    let save_dir = first_existing_save_dir(path)?;
    create_vn_draft(path, device_id, save_dir)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use temp_dir::TempDir;

    use super::{detect_kirikiri, detect_renpy, scan_games};

    #[test]
    fn detect_kirikiri_requires_xp3_and_maps_savedata() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let game_dir = temp_dir.path().join("MyGame");
        fs::create_dir(&game_dir).expect("game dir should be created");

        assert!(detect_kirikiri(&game_dir, "test-device").is_none());

        fs::File::create(game_dir.join("data.xp3")).expect("xp3 should be created");
        let draft =
            detect_kirikiri(&game_dir, "test-device").expect("kirikiri game should be detected");

        assert_eq!(draft.name, "MyGame");
        assert_eq!(draft.save_paths.len(), 1);
        let save_path = draft.save_paths[0]
            .paths()
            .expect("detected VN path should be concrete")
            .get("test-device")
            .expect("save path should exist");
        assert!(save_path.ends_with("savedata"));
    }

    #[test]
    fn detect_renpy_maps_game_saves() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let game_dir = temp_dir.path().join("RenpyGame");
        fs::create_dir(&game_dir).expect("game dir should be created");

        let sub_game = game_dir.join("game");
        fs::create_dir(&sub_game).expect("renpy game dir should be created");
        fs::File::create(sub_game.join("archive.rpa")).expect("rpa should be created");

        let draft = detect_renpy(&game_dir, "test-device").expect("renpy game should be detected");
        let save_path = draft.save_paths[0]
            .paths()
            .expect("detected VN path should be concrete")
            .get("test-device")
            .expect("save path should exist");

        assert_eq!(draft.name, "RenpyGame");
        assert!(save_path.ends_with("game/saves") || save_path.ends_with("game\\saves"));
    }

    #[test]
    fn scan_games_only_checks_first_level_subdirs() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let root = temp_dir.path().join("root");
        fs::create_dir(&root).expect("root should be created");

        let game_dir = root.join("DetectedGame");
        fs::create_dir(&game_dir).expect("game dir should be created");
        fs::File::create(game_dir.join("data.xp3")).expect("xp3 should be created");

        let nested_parent = root.join("Nested");
        fs::create_dir(&nested_parent).expect("nested parent should be created");
        let nested_game = nested_parent.join("TooDeep");
        fs::create_dir(&nested_game).expect("nested game dir should be created");
        fs::File::create(nested_game.join("data.xp3")).expect("nested xp3 should be created");

        let results = scan_games(&[root.to_string_lossy().to_string()]);
        let names: Vec<_> = results.iter().map(|game| game.name.as_str()).collect();

        assert!(names.contains(&"DetectedGame"));
        assert!(!names.contains(&"TooDeep"));
    }
}
