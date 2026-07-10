use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use rgsm_core::config::{Config, set_config_local};

const CONFIG_FILE_NAME: &str = "GameSaveManager.config.json";

#[derive(Debug, Clone)]
pub struct ImportReport {
    pub source_config_path: PathBuf,
    pub source_backup_path: PathBuf,
    pub target_backup_path: PathBuf,
    pub games: usize,
    pub devices: usize,
    pub copied_backup_files: usize,
    pub skipped_backup_files: usize,
}

#[derive(Debug, Default)]
struct CopyStats {
    copied_files: usize,
    skipped_files: usize,
}

pub fn import_gui_profile(source: &Path, target_data_dir: &Path) -> Result<ImportReport> {
    let source_config_path = resolve_source_config_path(source)?;
    let target_config_path = target_data_dir.join(CONFIG_FILE_NAME);
    reject_same_config(&source_config_path, &target_config_path)?;

    let source_data_dir = source_config_path
        .parent()
        .ok_or_else(|| anyhow!("source config has no parent directory"))?;
    let source_config = read_config(&source_config_path)?;
    let target_config = rgsm_core::config::get_config().context("failed to read TUI config")?;

    let source_backup_path = resolve_profile_path(source_data_dir, &source_config.backup_path);
    let target_backup_path = resolve_profile_path(target_data_dir, &target_config.backup_path);
    let mut stats = CopyStats::default();
    copy_missing_tree(&source_backup_path, &target_backup_path, &mut stats)?;

    let merged = merge_gui_config(&source_config, &target_config);
    let games = merged.games.len();
    let devices = merged.devices.len();
    set_config_local(&merged).context("failed to write imported TUI config")?;

    Ok(ImportReport {
        source_config_path,
        source_backup_path,
        target_backup_path,
        games,
        devices,
        copied_backup_files: stats.copied_files,
        skipped_backup_files: stats.skipped_files,
    })
}

fn resolve_source_config_path(source: &Path) -> Result<PathBuf> {
    let path = if source.is_dir() {
        source.join(CONFIG_FILE_NAME)
    } else {
        source.to_path_buf()
    };
    if !path.is_file() {
        bail!("GUI config not found at {}", path.display());
    }
    Ok(path)
}

fn reject_same_config(source: &Path, target: &Path) -> Result<()> {
    let source = source
        .canonicalize()
        .with_context(|| format!("failed to resolve source config path {}", source.display()))?;
    let target = target
        .canonicalize()
        .with_context(|| format!("failed to resolve target config path {}", target.display()))?;
    if source == target {
        bail!(
            "GUI source config and TUI target config are the same file: {}",
            source.display()
        );
    }
    Ok(())
}

fn read_config(path: &Path) -> Result<Config> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("failed to parse {}", path.display()))
}

fn resolve_profile_path(data_dir: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        data_dir.join(path)
    }
}

fn merge_gui_config(source: &Config, target: &Config) -> Config {
    let mut merged = target.clone();
    merged.games = source.games.clone();
    merged.devices = source.devices.clone();
    merged.settings.cloud_settings = source.settings.cloud_settings.clone();
    merged.settings.vn_scan_dirs = source.settings.vn_scan_dirs.clone();
    merged.settings.compression_preset = source.settings.compression_preset;
    merged.settings.compute_archive_hash = source.settings.compute_archive_hash;
    merged.settings.verify_archive_before_apply = source.settings.verify_archive_before_apply;
    merged.settings.default_delete_before_apply = source.settings.default_delete_before_apply;
    merged.settings.max_extra_backup_count = source.settings.max_extra_backup_count;
    merged
}

fn copy_missing_tree(source: &Path, target: &Path, stats: &mut CopyStats) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }
    if !source.is_dir() {
        bail!("backup path is not a directory: {}", source.display());
    }

    fs::create_dir_all(target).with_context(|| format!("failed to create {}", target.display()))?;
    for entry in fs::read_dir(source)
        .with_context(|| format!("failed to read directory {}", source.display()))?
    {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_missing_tree(&source_path, &target_path, stats)?;
        } else if file_type.is_file() {
            copy_missing_file(&source_path, &target_path, stats)?;
        }
    }
    Ok(())
}

fn copy_missing_file(source: &Path, target: &Path, stats: &mut CopyStats) -> Result<()> {
    if target.exists() {
        stats.skipped_files += 1;
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(source, target).with_context(|| {
        format!(
            "failed to copy backup file from {} to {}",
            source.display(),
            target.display()
        )
    })?;
    stats.copied_files += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use rgsm_core::backup::Game;
    use rgsm_core::device::Device;

    use super::*;

    fn game(name: &str) -> Game {
        Game {
            name: name.to_string(),
            storage_key: name.to_ascii_lowercase(),
            save_paths: Vec::new(),
            game_paths: Default::default(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            ludusavi_meta: None,
            device_bindings: Default::default(),
        }
    }

    #[test]
    fn merge_keeps_tui_profile_specific_fields() {
        let mut source = Config {
            backup_path: "gui-save-data".to_string(),
            ..Default::default()
        };
        source.games.push(game("Source"));
        source.devices.insert("device-a".into(), Device::default());
        source.settings.vn_scan_dirs = vec!["D:\\VN".into()];
        source.settings.compute_archive_hash = true;

        let mut target = Config {
            backup_path: "tui-save-data".to_string(),
            ..Default::default()
        };
        target.quick_action.enable_sound = false;

        let merged = merge_gui_config(&source, &target);

        assert_eq!(merged.backup_path, "tui-save-data");
        assert_eq!(merged.games.len(), 1);
        assert_eq!(merged.devices.len(), 1);
        assert_eq!(merged.settings.vn_scan_dirs, vec!["D:\\VN"]);
        assert!(merged.settings.compute_archive_hash);
        assert!(!merged.quick_action.enable_sound);
    }

    #[test]
    fn relative_backup_path_resolves_under_profile() {
        assert_eq!(
            resolve_profile_path(Path::new("profile"), "save_data"),
            PathBuf::from("profile").join("save_data")
        );
    }
}
