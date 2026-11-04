---
title: 数据备份与迁移
---

# 数据备份与迁移

## 整体备份

1. 从托盘完全退出存档管理器。
2. 复制 `GameSaveManager.config.v2/` 配置目录。
3. 复制本地备份目录，默认位于程序数据目录下的 `save_data`。

在游戏右上角菜单中选择 **打开备份文件夹**，可以找到实际位置。

## 数据文件

| 位置 | 内容 |
| --- | --- |
| `GameSaveManager.config.v2/` | 游戏、设备、连接信息和界面设置 |
| `Backups.json` | 各游戏的存档记录、描述和进度关系 |
| `.7z`、`.zip` | 存档文件，新版使用 `.7z` |
| `extra_backup/` | 本机的额外备份 |

配置目录包含云端凭据，请妥善保管。

## 程序数据目录中的其他文件

| 位置 | 内容 | 能否删除 |
| --- | --- | --- |
| `GameSaveManager.config.json.bak` | 升级前的旧配置 | 只在回退旧版时需要，改回 `GameSaveManager.config.json` 即可 |
| `GameSaveManager.config.json.backup.0` 至 `.4` | 保存设置前自动留下的配置备份，用于恢复配置 | 可以，但会失去可恢复的配置 |
| `GameSaveManager.host.json` | 本机 API 的端口和 Token | 可以，下次启动重新生成，Token 会变 |
| `ludusavi_manifest.yaml`、`ludusavi_manifest.meta.json` | 手动更新的游戏识别清单 | 可以，之后使用程序内置清单 |
| `log/`、`save_data/sync_state.json` | 旧版遗留 | 可以 |
| `save_data/.rgsm-upgrade/` | 本地快照升级的记录和原件 | 请在 **设置 → 备份设置 → 本地快照升级** 中点击 **清理原件**，不要手动删除 |

日志位于 `%LOCALAPPDATA%\com.game-save-manager\logs`，也可以在 **设置 → 通用 → 打开日志** 中打开。

## 换设备

多台设备共同使用时，按[云同步流程](../cloud/progress.mdx)连接云库，并设置本机存档位置。

离线迁移时，复制完整配置和备份目录，启动后调整存档路径。恢复前请另存一份当前数据。
