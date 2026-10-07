---
title: 读取备份格式
---

新版备份使用标准 7z 容器。程序读取 `_rgsm/manifest.json`，手动恢复时阅读 `RESTORE.txt`。TXT 的排版和措辞可以变化，不作为程序接口。

## 格式版本 6

Manifest 是 UTF-8 JSON，字段使用 camelCase。先检查顶层 `version`：本文说明版本 `6`。不要将未知版本按当前格式恢复；同一版本中可以忽略不认识的附加字段。

| 字段 | 含义 |
| --- | --- |
| `identity.gameId` / `gameName` | 游戏实例的标识 / 备份时的名称 |
| `identity.snapshotId` | 备份标识；不要从压缩文件名推导 |
| `identity.createdAt` | Unix 时间戳，单位毫秒；可能缺省 |
| `identity.legacyLocalTime` | 旧备份记录的本地时间，时区未知；可能缺省 |
| `identity.deviceId` / `parent` | 来源设备 / 父备份标识；可能缺省 |
| `identity.description` / `createdBy` | 备注 / 创建来源，例如 `Manual`、`Timer` |
| `identity.recoveredMetadata` | `true` 表示转换旧备份时重建的信息；缺省为 `false` |
| `identity.locations` | 存档条目的 `saveUnitId` 与原始路径表达式 `expression` |
| `groups` | 实际捕获的数据分组；一个存档条目可能匹配多个分组 |
| `groups[].id` / `saveUnitId` | 分组标识 / 所属存档条目标识；通过标识关联，不能依赖数组顺序 |
| `groups[].archivePath` | 分组在压缩包内的相对路径，分隔符为 `/` |
| `groups[].kind` | `file`、`directory` 或 `registry`；注册表数据为标准 `.reg` 文件 |
| `groups[].relativePath` / `relativeExpression` | 匹配项相对逻辑存档根目录的路径 / 保留变量的表达式；后者可能缺省 |
| `groups[].sourcePathDiagnostic` | 捕获时的原始位置，仅供参考；可能缺省 |
| `groups[].deleteBeforeApply` | 此条目的恢复前删除设置 |

最小读取示例：

```python
import json
from pathlib import Path

# 用支持 7z 的工具安全解压到独立目录后读取。
root = Path("extracted-backup")
manifest = json.loads((root / "_rgsm/manifest.json").read_text(encoding="utf-8"))
if manifest["version"] != 6:
    raise ValueError("Unsupported backup format")
for group in manifest["groups"]:
    print(group["saveUnitId"], group["kind"], group["archivePath"])
```

`archivePath` 指向实际数据；目录分组包括其子目录和文件。同名条目的压缩包路径会带数字后缀，必须使用 Manifest 中的路径，不要根据原始文件名重新拼接。

原始路径属于来源设备。在其他设备恢复时，应使用该设备对应的存档位置；`sourcePathDiagnostic` 不是可直接执行的恢复目标。`_rgsm` 和 `RESTORE.txt` 是备份元数据，不属于游戏存档。

历史 ZIP / 7z 可能没有此 Manifest，应先通过软件的本地备份升级转换。旧版信息不足时需要用户关联存档条目，不能仅按同名文件猜测。
