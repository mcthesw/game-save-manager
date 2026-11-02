---
title: Backing up and moving your data
---

# Backing up and moving your data

## Make a complete backup

1. Quit Game Save Manager from the system tray.
2. Copy the `GameSaveManager.config.v2/` configuration directory.
3. Copy the local backup directory, which defaults to `save_data` inside the application data directory.

Use **Open backup folder** in a game's top-right menu to find its location.

## Data files

| Location | Contents |
| --- | --- |
| `GameSaveManager.config.v2/` | Games, devices, connection details, and interface settings |
| `Backups.json` | Each game's snapshot records, descriptions, and progress relationships |
| `.7z`, `.zip` | Snapshot archives; new versions use `.7z` |
| `extra_backup/` | Extra backups on this device |

The configuration directory contains cloud credentials. Store it securely.

## Other files in the application data directory

| Location | Contents | Can it be deleted? |
| --- | --- | --- |
| `GameSaveManager.config.json.bak` | The configuration from before the upgrade | Only needed to roll back to an older version: rename it back to `GameSaveManager.config.json` |
| `GameSaveManager.config.json.backup.0` to `.4` | Configuration backups made automatically before settings are saved, used to restore the configuration | Yes, but you lose those restore points |
| `GameSaveManager.host.json` | The local API port and token | Yes. It is regenerated at the next start, with a new token |
| `ludusavi_manifest.yaml`, `ludusavi_manifest.meta.json` | The game detection list you updated manually | Yes. The built-in list is used afterwards |
| `log/`, `save_data/sync_state.json` | Left over by older versions | Yes |
| `save_data/.rgsm-upgrade/` | Records and originals of the older-backup upgrade | Use **Clean up originals** in **Settings → Backup settings → Upgrade older backups**; do not delete it by hand |

Logs are in `%LOCALAPPDATA%\com.game-save-manager\logs`. You can also open them from **Settings → General → Open log**.

## Move to another device

For shared use across devices, follow the [cloud sync workflow](../cloud/progress.mdx) and set each device's save locations.

For an offline move, copy the complete configuration and backup directories, then adjust save paths after starting the application. Keep a separate copy of your current data before restoring.
