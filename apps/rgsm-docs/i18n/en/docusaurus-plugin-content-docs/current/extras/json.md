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

## Move to another device

For shared use across devices, follow the [cloud sync workflow](../cloud/progress.mdx) and set each device's save locations.

For an offline move, copy the complete configuration and backup directories, then adjust save paths after starting the application. Keep a separate copy of your current data before restoring.
