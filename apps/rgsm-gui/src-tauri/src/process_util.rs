//! Shared process discovery helpers for GUI-only features.
//!
//! Process monitor integrations need to enumerate running processes and match
//! them against a configured executable name.

use rgsm_core::backup::Game;
use rgsm_core::config::{GameAutomationSettings, GameAutomationSettingsDraft};
use rgsm_core::device::get_current_device_id;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RunningProcessOption {
    pub name: String,
    pub label: String,
    pub count: u32,
}

/// Normalize a raw process/executable string into a comparable file name.
///
/// Strips surrounding quotes and directories, then lowercases, so that values
/// like `"C:\Games\ACOrigins.exe"` and `acorigins.exe` compare equal.
pub fn normalize_process_name(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('"');
    let file_name = trimmed.rsplit(['/', '\\']).next().unwrap_or(trimmed);
    file_name.trim().to_ascii_lowercase()
}

/// Resolve the executable name to watch for a game: the explicitly configured
/// process name, falling back to the launch path's file name.
pub fn process_name_for_game(game: &Game, automation: &GameAutomationSettings) -> Option<String> {
    let explicit = normalize_process_name(&automation.process_name);
    if !explicit.is_empty() {
        return Some(explicit);
    }

    let launch_path = game.game_paths.get(get_current_device_id())?;
    let normalized = normalize_process_name(launch_path);
    (!normalized.is_empty()).then_some(normalized)
}

pub fn process_name_for_game_draft(
    game: &Game,
    automation: &GameAutomationSettingsDraft,
) -> Option<String> {
    let explicit = normalize_process_name(&automation.process_name);
    if !explicit.is_empty() {
        return Some(explicit);
    }

    let launch_path = game.game_paths.get(get_current_device_id())?;
    let normalized = normalize_process_name(launch_path);
    (!normalized.is_empty()).then_some(normalized)
}

pub fn validate_process_target(
    game: &Game,
    automation: &GameAutomationSettingsDraft,
) -> anyhow::Result<()> {
    if !automation.has_process_triggers() || process_name_for_game_draft(game, automation).is_some()
    {
        return Ok(());
    }

    Err(anyhow::anyhow!(
        "Process monitor target is required when process triggers are enabled"
    ))
}

/// Compare an actual process name against an expected one, tolerating a missing
/// `.exe` suffix on the expected value (common when typed manually).
pub fn process_name_matches(actual: &str, expected: &str) -> bool {
    actual == expected || (!expected.ends_with(".exe") && actual == format!("{expected}.exe"))
}

/// Whether the expected process is present in a set of normalized process names.
pub fn process_is_running(processes: &HashSet<String>, expected: &str) -> bool {
    let expected = normalize_process_name(expected);
    if expected.is_empty() {
        return false;
    }
    processes
        .iter()
        .any(|actual| process_name_matches(actual, &expected))
}

/// The set of normalized names of all currently running processes.
pub fn running_process_names() -> anyhow::Result<HashSet<String>> {
    Ok(process_entries()?.into_values().collect())
}

/// Running process names formatted for the GUI process selector.
pub fn list_running_processes() -> anyhow::Result<Vec<RunningProcessOption>> {
    let mut processes: Vec<_> = process_name_counts()?
        .into_iter()
        .map(|(name, count)| RunningProcessOption {
            label: if count > 1 {
                format!("{name} x{count}")
            } else {
                name.clone()
            },
            name,
            count,
        })
        .collect();
    processes.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(processes)
}

fn process_name_counts() -> anyhow::Result<HashMap<String, u32>> {
    let mut counts = HashMap::new();
    for name in process_entries()?.into_values() {
        *counts.entry(name).or_default() += 1;
    }
    Ok(counts)
}

#[cfg(windows)]
mod windows_impl {
    use std::collections::HashMap;
    use std::mem::{size_of, zeroed};

    use anyhow::Context;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    /// Map of process id -> normalized executable name for all running processes.
    pub fn process_entries() -> anyhow::Result<HashMap<u32, String>> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error()).context("CreateToolhelp32Snapshot failed");
        }

        let mut entries = HashMap::new();
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..unsafe { zeroed() }
        };

        let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) };
        while ok != 0 {
            if let Some(name) = process_entry_name(&entry) {
                entries.insert(entry.th32ProcessID, name);
            }
            ok = unsafe { Process32NextW(snapshot, &mut entry) };
        }

        unsafe {
            CloseHandle(snapshot);
        }
        Ok(entries)
    }

    fn process_entry_name(entry: &PROCESSENTRY32W) -> Option<String> {
        let end = entry
            .szExeFile
            .iter()
            .position(|ch| *ch == 0)
            .unwrap_or(entry.szExeFile.len());
        if end == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&entry.szExeFile[..end]).to_ascii_lowercase())
    }
}

#[cfg(windows)]
pub use windows_impl::process_entries;

#[cfg(target_os = "linux")]
mod linux_impl {
    use std::collections::HashMap;
    use std::path::Path;

    use anyhow::Context;

    use super::normalize_process_name;

    pub fn process_entries() -> anyhow::Result<HashMap<u32, String>> {
        let mut entries = HashMap::new();
        for entry in std::fs::read_dir("/proc").context("failed to read /proc")? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            else {
                continue;
            };

            if let Some(name) = process_name_from_proc_entry(&entry.path()) {
                entries.insert(pid, name);
            }
        }
        Ok(entries)
    }

    fn process_name_from_proc_entry(path: &Path) -> Option<String> {
        process_name_from_exe_link(path.join("exe").as_path())
            .or_else(|| process_name_from_cmdline(path.join("cmdline").as_path()))
    }

    fn process_name_from_exe_link(path: &Path) -> Option<String> {
        let target = std::fs::read_link(path).ok()?;
        let name = target.file_name()?.to_str()?;
        let normalized = normalize_process_name(name);
        (!normalized.is_empty()).then_some(normalized)
    }

    fn process_name_from_cmdline(path: &Path) -> Option<String> {
        let cmdline = std::fs::read(path).ok()?;
        process_name_from_cmdline_bytes(&cmdline)
    }

    fn process_name_from_cmdline_bytes(cmdline: &[u8]) -> Option<String> {
        let command = cmdline
            .split(|byte| *byte == 0)
            .find(|part| !part.is_empty())?;
        let normalized = normalize_process_name(&String::from_utf8_lossy(command));
        (!normalized.is_empty()).then_some(normalized)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn cmdline_fallback_preserves_long_process_names() {
            assert_eq!(
                process_name_from_cmdline_bytes(b"/opt/game/averyverylongprocessname\0--flag"),
                Some("averyverylongprocessname".to_string())
            );
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux_impl::process_entries;

#[cfg(all(not(windows), not(target_os = "linux")))]
mod ps_impl {
    use std::collections::HashMap;

    use anyhow::Context;

    use super::normalize_process_name;

    pub fn process_entries() -> anyhow::Result<HashMap<u32, String>> {
        let output = std::process::Command::new("ps")
            .args(["-A", "-o", "pid=", "-o", "command="])
            .output()
            .context("failed to run ps")?;
        if !output.status.success() {
            return Err(anyhow::anyhow!("ps exited with {}", output.status));
        }

        let stdout = String::from_utf8(output.stdout).context("ps output was not UTF-8")?;
        Ok(stdout
            .lines()
            .filter_map(process_entry_from_ps_line)
            .collect())
    }

    fn process_entry_from_ps_line(line: &str) -> Option<(u32, String)> {
        let mut parts = line.trim_start().splitn(2, char::is_whitespace);
        let pid = parts.next()?.parse::<u32>().ok()?;
        let command = parts.next()?.split_whitespace().next()?;
        let normalized = normalize_process_name(command);
        (!normalized.is_empty()).then_some((pid, normalized))
    }
}

#[cfg(all(not(windows), not(target_os = "linux")))]
pub use ps_impl::process_entries;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_process_paths_to_lowercase_file_name() {
        assert_eq!(
            normalize_process_name(r#""C:\Games\ACOrigins.exe""#),
            "acorigins.exe"
        );
        assert_eq!(
            normalize_process_name("/home/user/Games/ACOrigins.exe"),
            "acorigins.exe"
        );
    }

    #[test]
    fn process_match_accepts_missing_exe_suffix() {
        let mut processes = HashSet::new();
        processes.insert("acorigins.exe".to_string());

        assert!(process_is_running(&processes, "ACOrigins"));
        assert!(process_is_running(&processes, "ACOrigins.exe"));
        assert!(!process_is_running(&processes, "Other.exe"));
    }

    #[test]
    fn process_target_validation_rejects_trigger_without_target() {
        let game = test_game();
        let automation = automation_draft("", true);

        assert!(validate_process_target(&game, &automation).is_err());
    }

    #[test]
    fn process_target_validation_accepts_explicit_target() {
        let game = test_game();
        let automation = automation_draft("Game.exe", true);

        assert!(validate_process_target(&game, &automation).is_ok());
    }

    #[test]
    fn process_target_validation_accepts_current_device_launch_path() {
        let mut game = test_game();
        game.game_paths.insert(
            get_current_device_id().to_string(),
            r#"C:\Games\Game.exe"#.to_string(),
        );
        let automation = automation_draft("", true);

        assert!(validate_process_target(&game, &automation).is_ok());
    }

    #[test]
    fn process_target_validation_allows_disabled_trigger_without_target() {
        let game = test_game();
        let automation = automation_draft("", false);

        assert!(validate_process_target(&game, &automation).is_ok());
    }

    fn automation_draft(process_name: &str, on_process_start: bool) -> GameAutomationSettingsDraft {
        GameAutomationSettingsDraft {
            process_name: process_name.to_string(),
            on_process_start,
            on_process_exit: false,
            in_process_interval_secs: None,
        }
    }

    fn test_game() -> Game {
        Game {
            name: "Game".to_string(),
            storage_key: "game".to_string(),
            save_paths: Vec::new(),
            game_paths: HashMap::new(),
            next_save_unit_id: 0,
            cloud_sync_enabled: true,
            auto_backup: None,
            ludusavi_meta: None,
            store_user_ids: HashMap::new(),
        }
    }
}
