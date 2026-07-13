# 路线图 / Roadmap 🗺️

本文档记录了 Game Save Manager 的未来开发方向和架构决策。

## V2.0

### 存档系统升级

- [x] **压缩预设**：用户可选的压缩级别（仅存储 / 快速 / 标准 / 极限），使用 Zstd 替代 BZip2
- [x] **V2 存档格式**：`RGSM_ARCHIVE_V2` 头 + JSON 元数据，index-prefixed 条目布局
- [x] **ArchiveBackend trait**：可扩展的存档后端抽象（当前实现：ZipBackend、SevenZBackend）
- [x] **同名文件支持**：不同存档单元中的同名文件不再冲突
- [x] **生命周期 Hook 基础**：围绕快照创建/删除、恢复前、恢复后、配置保存、同步完成等事件提供 typed context + 内置 hooks 的组合式管线
- [x] **Archive V4**：新 Snapshot 使用标准 7z、内部 Capture Manifest 和格式感知的本地/云端路径；旧 ZIP 永久保持可读
- [ ] **Archive-stream Hook trait**：在压缩/解压流内部插入自定义逻辑，与业务生命周期 hooks 分层
- [x] **整体存档 hash 与恢复前校验**：可选生成 archive hash，并在恢复前阻止损坏存档被应用
- [x] **Unix 权限保存**：Archive V4 使用标准 7z 属性保存和恢复完整 POSIX mode（不含 UID/GID、ACL、xattr）
- [x] **外部扩展附件目录**：为快照扩展保留 `save_data/<game>/extra_info/<extension>/` 路径边界，扩展目录自行维护 manifest，避免频繁读取 zip 或膨胀 `Backups.json`
- [ ] **快照内嵌扩展数据**：对确实需要随 archive 原子迁移的附件，放入保留目录或后续 archive-stream 扩展中

### 云同步大幅优化

- [x] **同步状态可视化基础**：记录配置与游戏级别的同步结果、待处理状态与最近同步时间，并在设置页 Overview 展示
- [x] **多设备位置跟踪**：以“每个设备自己的当前位置”替代全局单一 head，兼容旧数据迁移
- [x] **并行分支/同设备分叉识别**：区分“本设备领先/落后”“不同设备并行分支”“同设备不兼容分叉”，避免误判
- [ ] **错误恢复**：操作失败时的重试和回滚机制
- [ ] **冲突解决 UX**：对需要人工处理的同步状态提供明确的玩家操作指引，而不是仅返回技术结论
- [ ] **增量同步**：仅传输变更的文件，减少带宽消耗
- [ ] **新设备接入引导**：首次在新设备上创建快照时，明确选择“从现有进度继续”或“开始新的进度线”
- [ ] **多设备支持增强**：继续完善取消、批量操作、后台队列和跨设备体验一致性

### 游戏识别与集成

- [ ] **Steam 存档自动识别优化**：改善现有的 Ludusavi manifest 集成
- [ ] **游戏引擎存档扫描工具**：将当前硬编码的 VN 引擎扫描整理成可扩展的扫描工具，后续考虑通过插件或其他扩展方式为特定引擎新增扫描规则
- [ ] **Playnite / Vnite 插件**：第三方游戏库集成导入

### 分发

- [ ] **Winget / Scoop 支持**：通过包管理器安装和更新（欢迎 PR）

### 工程优化

- [x] **Workspace 拆分**：将项目整理为 workspace 结构（`apps/rgsm-gui`、`apps/rgsm-cli`、`crates/rgsm-core`、`crates/rgsm-ffi`），为 CLI 工具和 FFI 集成提供基础

## V3.0（远期展望）

- [ ] 存档导入 / 导出
- [ ] 高级插件系统：插件可接入 lifecycle hooks，并按读取频率选择附件层级：archive 内保留目录（随压缩包迁移、低频读取）、`Backups.json` 简短字段（如 hash/小型索引）、`extra_info/<extension>/` 外部扩展目录（截图等可独立读写的大对象；当前截图预览是 GUI 内置扩展）
- [ ] 存档云共享平台（可能需要脚本系统支持存档适配）
