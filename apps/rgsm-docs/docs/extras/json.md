# Json文件说明

## GameSaveManager.config.json

```json5
{
  "version": "1.8.0", // 配置文件的版本
  "backup_path": "./save_data", // 备份文件存放路径，暂时不可变，不要更改
  "games": [ // 存放所有游戏的数组
    {
      "name": "测试游戏", // 当前游戏的名字
      "save_paths": [ // 存放当前游戏所有存档文件/文件夹的路径
        {
          "id": 0, // 存档单元的唯一标识符 (稳定ID，不会因增删而改变)
          "unit_type": "Folder", // 存档的类型: Folder (文件夹), File (文件), 或 WinRegistry (Windows注册表)
          "paths": { // 存档的路径 (按设备ID映射)
            "device_id_1": "X:\\Tests\\存档案例1\\文件夹案例1", // 示例设备1的路径
            "device_id_2": "Y:\\GameSaves\\测试游戏\\文件夹案例1"  // 示例设备2的路径
          },
          "delete_before_apply": false // 应用存档时是否删除原存档
        },
        {
          "id": 1, // 存档单元的唯一标识符 (稳定ID，不会因增删而改变)
          "unit_type": "File",
          "paths": {
            "device_id_1": "X:\\Tests\\存档案例1\\文件图片1.jpg",
            "device_id_2": "Y:\\GameSaves\\测试游戏\\文件图片1.jpg"
          },
          "delete_before_apply": true
        },
        {
          "id": 2,
          "unit_type": "WinRegistry", // Windows注册表备份
          "paths": {
            "device_id_1": "HKEY_CURRENT_USER\\SOFTWARE\\GameCompany\\TestGame"
          },
          "delete_before_apply": false
        }
      ],
      "game_paths": { // 游戏的启动路径 (按设备ID映射)
        "device_id_1": "X:\\Games\\测试游戏\\start.exe", // 示例设备1的游戏启动路径
        "device_id_2": "" // 示例设备2的游戏启动路径 (可为空)
      },
      "next_save_unit_id": 3, // 下一个存档单元的ID计数器
      "cloud_sync_enabled": true // 是否为此游戏启用云同步 (默认: true)
    }
    // ... 更多游戏
  ],
  "settings": { // 软件的设置信息
    "prompt_when_not_described": true, // 未描述存档时是否提示
    "extra_backup_when_apply": true, // 应用存档时是否额外备份 (默认: true)
    "show_edit_button": false, // 是否显示编辑按钮（已弃用）
    "prompt_when_auto_backup": true, // 自动备份时是否提示
    "exit_to_tray": true, // 是否退出到托盘 (默认: true)
    "cloud_settings": { // 云同步的设置
      "always_sync": false, // [已弃用] 随时同步，已被每个游戏的 cloud_sync_enabled 替代
      "auto_sync_interval": 0, // 自动同步的间隔 (单位：分钟，0为禁用)
      "root_path": "/game-save-manager", // 云端的根路径
      "backend": { // 云同步后端的设置
        "type": "WebDAV", // 支持 WebDAV, S3, Disabled
        "endpoint": "https://cloud.example.com/dav",
        "username": "user",
        "password": "password"
        // S3 类型时使用以下字段:
        // "type": "S3",
        // "endpoint": "https://s3.amazonaws.com",
        // "bucket": "my-bucket",
        // "region": "us-east-1",
        // "access_key_id": "AKIA...",
        // "secret_access_key": "..."
      },
      "max_concurrency": 1 // 云同步最大并发数
    },
    "locale": "zh_SIMPLIFIED", // 语言: en_US, zh_SIMPLIFIED 等
    "default_delete_before_apply": false, // 新游戏是否默认先删除再恢复
    "default_expend_favorites_tree": false, // 是否默认展开收藏树
    "home_page": "/", // 默认主页路径
    "log_to_file": true, // 日志是否记录到文件
    "add_new_to_favorites": false, // 新游戏是否自动加入收藏夹
    "save_list_expand_behavior": "always_closed", // 存档列表展开行为: always_open, always_closed, remember_last
    "save_list_last_expanded": false, // 上次展开状态 (配合 remember_last 使用)
    "max_auto_backup_count": 0, // 最大自动备份数量 (0 = 不限)
    "max_extra_backup_count": 5, // 最大额外备份数量 (默认5，0 = 不限)
    "appearance": { // 外观设置
      "custom_font_enabled": false, // 是否启用自定义字体
      "ui_font_family": "MiSans" // 自定义字体名称
    },
    "compression_preset": "Standard", // 压缩预设: Store, Fast, Standard, Best
    "compute_archive_hash": false, // 备份时是否计算存档哈希 (XXH3)
    "verify_archive_before_apply": false // 恢复前是否验证存档完整性
  },
  "favorites": [ // 收藏夹树
    {
      "node_id": "uuid_string_1", // 节点ID，使用UUID4
      "label": "游戏或文件夹名", // 游戏名或文件夹名，用于导航和显示
      "is_leaf": true, // true代表是游戏，false代表是文件夹
      "children": null // 如果 is_leaf 为 false, 这里是子节点数组 Array<FavoriteNode>
    }
    // ... 更多收藏项或文件夹
  ],
  "quick_action": { // 快捷操作相关设置
    "quick_action_game": { /* 与 games[] 中的结构相同 */ },
    "hotkeys": { // 快捷键定义
      "apply": ["CONTROL", "ALT", "F1"], // 应用存档的快捷键
      "backup": ["SHIFT", "CONTROL", "S"] // 备份存档的快捷键
    },
    "enable_sound": true, // 是否启用快捷操作音效
    "enable_notification": true, // 是否启用快捷操作通知
    "sounds": { // 自定义音效
      "success": { "kind": "default" }, // "default" 使用内置音效, 或 {"kind": "file", "path": "..."}
      "failure": { "kind": "default" }
    }
  },
  "devices": { // 设备列表 (设备ID -> 设备信息)
    "device_id_1": { // 设备的唯一ID (通常是UUID)
      "id": "device_id_1", // 设备唯一ID
      "name": "我的电脑" // 设备名称
    },
    "device_id_2": {
      "id": "device_id_2",
      "name": "笔记本"
    }
    // ... 更多设备
  }
}
```

## save_data/游戏名/Backups.json

```json5
{
  "name": "测试游戏", // 游戏名
  "backups": [ // 存档的备份数组
    {
      "date": "2024-02-19_00-20-51", // 备份的日期，作为唯一标识符
      "describe": "初始状态", // 备份的描述
      "path": "./save_data\\测试游戏\\2024-02-19_00-20-51.zip", // 存放备份的路径
      "size": 5591, // 备份文件体积，单位是字节 (Byte)
      "parent": null, // 父快照的日期 (用于快照分支，null 表示根节点)
      "archive_hash": "a1b2c3d4e5f6", // 存档的 XXH3 哈希值 (可选，用于完整性校验)
      "device_id": "device-uuid-string" // 创建此快照的设备ID (可选)
    },
    {
      "date": "2024-02-20_10-30-00",
      "describe": "击败Boss后",
      "path": "./save_data\\测试游戏\\2024-02-20_10-30-00.zip",
      "size": 6200,
      "parent": "2024-02-19_00-20-51", // 从"初始状态"快照分支而来
      "archive_hash": null,
      "device_id": null
    }
  ],
  "device_heads": { // 每个设备的当前快照指针 (用于多设备同步)
    "device-uuid-1": "2024-02-20_10-30-00", // 设备1当前指向的快照
    "device-uuid-2": "2024-02-19_00-20-51"  // 设备2当前指向的快照
  },
  "legacy_head": null, // 旧版单一 head 指针 (向后兼容，已被 device_heads 替代)
  "sync_version": 3, // 同步版本号 (单调递增，用于冲突检测)
  "last_sync_device": "device-uuid-1", // 上次同步的设备ID (可选)
  "last_sync_timestamp": "2024-02-20T10:30:00Z" // 上次同步的时间 (ISO 8601, 可选)
}
```

## 额外备份 (Extra Backup)

当启用"应用存档时进行额外备份"后，每次恢复存档前会自动创建当前存档的安全备份。

**存放位置**: `{backup_path}/{游戏名}/extra_backup/`

**文件命名**: `Overwrite_YYYY-MM-DD_HH-MM-SS.zip`

额外备份的元数据不存储在 JSON 文件中，而是直接从文件系统读取：

- `date`: 文件名（不含扩展名），例如 `Overwrite_2024-02-19_00-20-51`
- `size`: 文件大小（字节）
- `modified_time_ms`: 文件修改时间（Unix 毫秒时间戳）

可通过设置中的 `max_extra_backup_count` 控制保留数量，超出限制时自动删除最旧的备份。

## 压缩格式与存档版本

### 压缩预设

应用支持以下压缩预设，可在设置中切换：

| 预设 | 压缩算法 | 压缩级别 | 说明 |
|------|----------|----------|------|
| Store | 无压缩 | - | 最快速度，文件最大 |
| Fast | Deflate | 1 | 较快压缩，适合大存档 |
| Standard（默认） | Zstd | 3 | 速度与压缩率的最佳平衡 |
| Best | Zstd | 19 | 最高压缩率，速度较慢 |

> 注意：旧版使用 BZip2 压缩的存档仍然可以正常读取和恢复。

### 存档版本

存档格式经历了三个版本的演进：

| 版本 | 标识 | 时间戳 | 文件布局 | 说明 |
|------|------|--------|----------|------|
| Legacy | 无标识 | UTC | 扁平结构 | 早期版本 (v1.0 之前) |
| V1 | `RGSM_TS_MODE=LOCAL_V1` | 本地时间 | 扁平结构 | 修正时间戳问题 |
| V2 | `RGSM_ARCHIVE_V2` + JSON | 本地时间 | 索引前缀 (`{id}/路径`) | 当前版本，支持同名文件 |

V2 存档在 ZIP 文件注释中存储 JSON 元数据：

```
RGSM_ARCHIVE_V2
{"version":2,"compression":"zstd:3","source_fingerprint":"可选的指纹哈希"}
```

- `version`: 存档格式版本号
- `compression`: 使用的压缩算法标识 (如 `stored`, `deflate`, `zstd:3`, `zstd:19`)
- `source_fingerprint`: 源文件的 XXH3 指纹 (用于自动备份去重，可选)

> V2 格式使用存档单元 ID 作为 ZIP 条目的前缀（如 `0/save.dat`、`1/config.ini`），因此不同存档单元可以包含同名文件，不会发生冲突。

## 配置文件备份

应用会在每次保存配置时自动轮转备份配置文件：

- `GameSaveManager.config.json.backup.0` — 最近的备份
- `GameSaveManager.config.json.backup.1`
- ...
- `GameSaveManager.config.json.backup.4` — 最旧的备份

最多保留 5 份备份，新的备份会将旧的向后推移。如果配置文件损坏，可以通过重命名备份文件来恢复。
