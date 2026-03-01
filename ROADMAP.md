# 路线图 / Roadmap 🗺️

本文档记录了 Game Save Manager 的未来开发方向和架构决策。

## V2.0

### 存档系统升级

- [x] **压缩预设**：用户可选的压缩级别（仅存储 / 快速 / 标准 / 极限），使用 Zstd 替代 BZip2
- [x] **V2 存档格式**：`RGSM_ARCHIVE_V2` 头 + JSON 元数据，index-prefixed 条目布局
- [x] **ArchiveBackend trait**：可扩展的存档后端抽象（当前实现：ZipBackend）
- [x] **同名文件支持**：不同存档单元中的同名文件不再冲突
- [ ] **`__rgsm__/` 元数据目录**：在 ZIP 内部存储结构化元数据
- [ ] **ArchiveHook trait**：程序内 hook 系统，在压缩/解压过程中插入自定义逻辑
- [ ] **校验和验证**：文件级校验 + 整体存档 hash，UI 提供验证备份完整性功能
- [ ] **Unix 权限保存**：利用 ZIP 格式原生的 Unix 权限字段保存和恢复文件权限
- [ ] **快照截图**：在创建快照时嵌入游戏截图

### 云同步大幅优化

- [ ] **错误恢复**：操作失败时的重试和回滚机制
- [ ] **冲突处理**：多设备同步时的冲突检测与解决策略
- [ ] **同步状态可视化**：清晰展示哪些文件已同步、配置文件同步状态
- [ ] **增量同步**：仅传输变更的文件，减少带宽消耗
- [ ] **多设备支持增强**：改善多设备场景下的可靠性

### 游戏识别与集成

- [ ] **Steam 存档自动识别优化**：改善现有的 Ludusavi manifest 集成
- [ ] **Playnite / Vnite 插件**：第三方游戏库集成导入

### 分发

- [ ] **Winget / Scoop 支持**：通过包管理器安装和更新（欢迎 PR）

### 工程优化

- [ ] **Workspace 拆分**：将 `src-tauri/` 拆分为多个 crate，为 CLI 工具和 FFI 集成提供基础

## V3.0（远期展望）

- [ ] 存档导入 / 导出
- [ ] 高级插件系统（WASM 插件）
- [ ] 存档云共享平台（可能需要脚本系统支持存档适配）

---

## 架构决策记录

### 保留 ZIP 作为唯一容器格式

`zip` crate v5.1.1 在单个 ZIP 文件内支持多种压缩方法（Stored, Deflate, BZip2, Zstd）。切换到 7z/RAR/tar 会破坏云同步（约 30 处硬编码 `.zip` 引用），收益有限。通过在 ZIP 内部变换压缩方法，保持现有基础设施不变。

### Hook 数据存储策略

- **小元数据**（校验和、权限等）→ ZIP 内部 `__rgsm__/` 目录，随存档绑定
- **大文件**（截图）→ 同样放在 ZIP 内部，由压缩处理体积
- **元数据索引**（启用了哪些 hooks）→ `ArchiveMeta` JSON（ZIP comment）
- **完整性校验** → 整体存档级别 hash

### ArchiveHook 设计方向

程序内 trait hooks，零运行时开销：

- Hook 点：`on_file_written`, `on_archive_finished`, `on_file_extracted`, `on_archive_opened`
- 内置实现：`ChecksumHook`、`UnixPermissionHook`、`ScreenshotHook`
- 未来可迁移到 WASM 插件系统（V3），hook 接口保持兼容
