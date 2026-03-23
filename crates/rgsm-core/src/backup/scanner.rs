use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use log::{info, warn};

use crate::backup::{GameDraft, SaveUnitDraft, SaveUnitType};
use crate::device::get_current_device_id;

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

/// Scan provided directories (1-level deep) for supported Galgames
/// and return them as `GameDraft`s.
pub fn scan_directories(dirs: &[String]) -> Vec<GameDraft> {
    let mut results = Vec::new();
    let current_device_id = get_current_device_id().to_string();

    for dir_path in dirs {
        let path = Path::new(dir_path);
        if !path.is_dir() {
            warn!(
                target: "rgsm::backup::scanner",
                "Skipping non-directory scan target: {}",
                dir_path
            );
            continue;
        }

        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(err) => {
                warn!(
                    target: "rgsm::backup::scanner",
                    "Failed to read scan root {}: {}",
                    dir_path,
                    err
                );
                continue;
            }
        };

        for entry in entries.filter_map(Result::ok) {
            if let Ok(metadata) = entry.metadata() {
                if is_offline(&metadata) {
                    continue;
                }
            }

            let subdir = entry.path();
            if !subdir.is_dir() {
                continue;
            }

            if let Some(game_draft) = check_all_engines(&subdir, &current_device_id) {
                info!(
                    target: "rgsm::backup::scanner",
                    "Detected Galgame candidate: {}",
                    game_draft.name
                );
                results.push(game_draft);
            }
        }
    }

    results
}

fn check_all_engines(path: &Path, device_id: &str) -> Option<GameDraft> {
    check_kirikiri(path, device_id)
        .or_else(|| check_renpy(path, device_id))
        .or_else(|| check_rpg_maker_mv_mz(path, device_id))
        .or_else(|| check_rpg_maker_vx(path, device_id))
        .or_else(|| check_wolf_rpg(path, device_id))
        .or_else(|| check_generic_savedata(path, device_id))
}

fn get_candidate_names(path: &Path) -> Vec<String> {
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

    let generic_exes = [
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

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.filter_map(Result::ok) {
            let candidate = entry.path();
            if !candidate.is_file() {
                continue;
            }

            if let Some(ext) = candidate.extension() {
                if ext.to_string_lossy().eq_ignore_ascii_case("exe") {
                    if let Some(stem) = candidate.file_stem() {
                        let stem_str = stem.to_string_lossy().to_string();
                        let stem_lower = stem_str.to_lowercase();
                        let is_generic = generic_exes.iter().any(|generic| stem_lower.starts_with(generic));
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

    for save_dir in ["savedata", "SaveData", "save", "Save"] {
        let candidate = direct.join(save_dir);
        if candidate.is_dir() {
            return Some(candidate);
        }
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

    if let Ok(entries) = fs::read_dir(&base) {
        for entry in entries.filter_map(Result::ok) {
            let sub_path = entry.path();
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

fn create_draft(path: &Path, device_id: &str, save_subpath: &str) -> Option<GameDraft> {
    let name = path.file_name()?.to_string_lossy().to_string();
    let candidates = get_candidate_names(path);

    let save_path = if let Some(external_path) = find_external_save_path(&candidates) {
        external_path
    } else if save_subpath.is_empty() {
        path.to_path_buf()
    } else {
        path.join(save_subpath)
    };

    let mut paths = HashMap::new();
    paths.insert(device_id.to_string(), save_path.to_string_lossy().to_string());

    let mut game_paths = HashMap::new();
    game_paths.insert(device_id.to_string(), path.to_string_lossy().to_string());

    let save_unit = SaveUnitDraft {
        id: None,
        unit_type: SaveUnitType::Folder,
        paths,
        delete_before_apply: false,
        enabled: true,
    };

    Some(GameDraft {
        name,
        save_paths: vec![save_unit],
        game_paths,
    })
}

fn check_kirikiri(path: &Path, device_id: &str) -> Option<GameDraft> {
    let mut has_xp3 = false;

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.filter_map(Result::ok) {
            if let Ok(metadata) = entry.metadata() {
                if is_offline(&metadata) {
                    continue;
                }
            }

            let candidate = entry.path();
            if candidate.is_file() {
                if let Some(ext) = candidate.extension() {
                    if ext.to_string_lossy().eq_ignore_ascii_case("xp3") {
                        has_xp3 = true;
                        break;
                    }
                }
            }
        }
    }

    if !has_xp3 {
        return None;
    }

    create_draft(path, device_id, "savedata")
}

fn check_renpy(path: &Path, device_id: &str) -> Option<GameDraft> {
    let game_dir = path.join("game");
    if !game_dir.is_dir() {
        return None;
    }

    let mut has_rpa = false;
    if let Ok(entries) = fs::read_dir(&game_dir) {
        for entry in entries.filter_map(Result::ok) {
            if let Ok(metadata) = entry.metadata() {
                if is_offline(&metadata) {
                    continue;
                }
            }

            let candidate = entry.path();
            if candidate.is_file() {
                if let Some(ext) = candidate.extension() {
                    if ext.to_string_lossy().eq_ignore_ascii_case("rpa") {
                        has_rpa = true;
                        break;
                    }
                }
            }
        }
    }

    if !has_rpa {
        return None;
    }

    create_draft(path, device_id, "game/saves")
}

fn check_rpg_maker_mv_mz(path: &Path, device_id: &str) -> Option<GameDraft> {
    let has_www = path.join("www").is_dir();
    let has_package = path.join("package.json").is_file();

    if !has_www && !has_package {
        return None;
    }

    if has_www {
        create_draft(path, device_id, "www/save")
    } else {
        create_draft(path, device_id, "save")
    }
}

fn check_rpg_maker_vx(path: &Path, device_id: &str) -> Option<GameDraft> {
    let mut has_rgss = false;

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.filter_map(Result::ok) {
            if let Ok(metadata) = entry.metadata() {
                if is_offline(&metadata) {
                    continue;
                }
            }

            let candidate = entry.path();
            if candidate.is_file() {
                if let Some(ext) = candidate.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    if matches!(ext.as_str(), "rgss3a" | "rgss2a" | "rvdata2" | "rvdata") {
                        has_rgss = true;
                        break;
                    }
                }
            }
        }
    }

    if !has_rgss {
        return None;
    }

    create_draft(path, device_id, "")
}

fn check_wolf_rpg(path: &Path, device_id: &str) -> Option<GameDraft> {
    let has_data = path.join("Data").is_dir();
    let has_game_exe = path.join("Game.exe").is_file();
    let has_game_dat = path.join("Game.dat").is_file();

    if !has_data || (!has_game_exe && !has_game_dat) {
        return None;
    }

    create_draft(path, device_id, "Save")
}

fn check_generic_savedata(path: &Path, device_id: &str) -> Option<GameDraft> {
    let mut has_exe = false;

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.filter_map(Result::ok) {
            if let Ok(metadata) = entry.metadata() {
                if is_offline(&metadata) {
                    continue;
                }
            }

            let candidate = entry.path();
            if candidate.is_file() {
                if let Some(ext) = candidate.extension() {
                    if ext.to_string_lossy().eq_ignore_ascii_case("exe") {
                        has_exe = true;
                        break;
                    }
                }
            }
        }
    }

    if !has_exe {
        return None;
    }

    for save_dir in ["savedata", "SaveData", "save", "Save"] {
        if path.join(save_dir).is_dir() {
            return create_draft(path, device_id, save_dir);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::fs;

    use temp_dir::TempDir;

    use super::{check_kirikiri, check_renpy, scan_directories};

    #[test]
    fn check_kirikiri_requires_xp3_and_maps_savedata() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let game_dir = temp_dir.path().join("MyGame");
        fs::create_dir(&game_dir).expect("game dir should be created");

        assert!(check_kirikiri(&game_dir, "test-device").is_none());

        fs::File::create(game_dir.join("data.xp3")).expect("xp3 should be created");
        let draft = check_kirikiri(&game_dir, "test-device").expect("kirikiri game should be detected");

        assert_eq!(draft.name, "MyGame");
        assert_eq!(draft.save_paths.len(), 1);
        let save_path = draft.save_paths[0]
            .paths
            .get("test-device")
            .expect("save path should exist");
        assert!(save_path.ends_with("savedata"));
    }

    #[test]
    fn check_renpy_maps_game_saves() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let game_dir = temp_dir.path().join("RenpyGame");
        fs::create_dir(&game_dir).expect("game dir should be created");

        let sub_game = game_dir.join("game");
        fs::create_dir(&sub_game).expect("renpy game dir should be created");
        fs::File::create(sub_game.join("archive.rpa")).expect("rpa should be created");

        let draft = check_renpy(&game_dir, "test-device").expect("renpy game should be detected");
        let save_path = draft.save_paths[0]
            .paths
            .get("test-device")
            .expect("save path should exist");

        assert_eq!(draft.name, "RenpyGame");
        assert!(save_path.ends_with("game/saves") || save_path.ends_with("game\\saves"));
    }

    #[test]
    fn scan_directories_only_checks_first_level_subdirs() {
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

        let results = scan_directories(&[root.to_string_lossy().to_string()]);
        let names: Vec<_> = results.iter().map(|game| game.name.as_str()).collect();

        assert!(names.contains(&"DetectedGame"));
        assert!(!names.contains(&"TooDeep"));
    }
}
