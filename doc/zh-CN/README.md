# 开发者指南

## 简介

本文档为希望为 Game-save-manager 项目做出贡献的开发者提供指南。其中包括有关项目目标、架构和开发流程的信息。

## 如何在本地开发

### 环境配置

你需要预先安装好以下环境：

- [Node.js](https://nodejs.org/) 和 [pnpm](https://pnpm.io/)
- [Rust 编译环境](https://www.rust-lang.org/)和 Cargo

### 编辑器和插件

- Visual Studio Code（推荐）
  - Rust-analyzer
  - Tauri
  - Vue - Official
  - Element Plus Snippets
  - i18n Allay
- WebStorm
- RustRover

### 安装依赖

`pnpm i`

### 编译与开发

请参考`package.json`来了解指令

- `pnpm dev` 开发模式，一边预览一边开发
- `pnpm build` 编译打包，输出会存放在`src-tauri/target`

**提示**：设置环境变量 `NUXT_DEVTOOLS=true` 可启用 Nuxt DevTools，默认禁用以加快启动速度。

## 架构

该软件分为两个主要部分：

- 前端负责用户界面和交互。它使用 TypeScript 和 Vue3 编写
  - 使用 Element Plus 组件库
  - 使用 pinia 进行状态管理
  - 使用 vue-router 作为前端路由
  - 使用 vue-i18n 进行国际化
- 后端负责管理游戏存档文件。它使用 Rust 编写
  - 使用 opendal 来访问云存储
  - 云同步按职责拆分为：
    - `cloud_sync/backend.rs`：后端配置与带重试策略的 Operator 构建
    - `cloud_sync/transfer.rs`：统一流式上传/下载抽象（含 hook 扩展点）
    - `cloud_sync/utils.rs`：同步工作流编排
    - `cloud_sync/facade.rs`：供 IPC 调用的领域入口
  - 使用 serde 来序列化和反序列化数据
  - 使用 thiserror 和 anyhow 进行错误处理

### 后端分层约定

- 保持 `ipc_handler.rs` 为薄导出层。
- IPC 层不要直接构造 OpenDAL Operator，应调用 cloud-sync facade。
- 传输逻辑放在 `cloud_sync/transfer.rs`，流程编排放在 `cloud_sync/utils.rs`。

### Snapshot 相关数据与附件分层

Snapshot 除了 zip 压缩包本身，还可能需要挂接校验信息、生成索引、未来插件生成的上下文等数据。为避免把不同读写频率的数据混在一起，扩展数据按访问模式分三层：

- **压缩包内部**：适合随 Snapshot 一起迁移、低频读取、恢复时才需要的数据。未来扩展应使用 archive 内的保留目录存放，避免污染用户的 Save Unit 路径。
- **`Backups.json` 简短字段**：适合高频读取的小型元数据，例如 `archive_hash`、创建来源、轻量索引或 UI 需要直接排序/筛选的字段。这里不应放大对象或插件私有大块数据。
- **外部附件目录**：适合需要单独读写、可能频繁访问、体积较大的数据。当前约定放在 `save_data/<game>/extra_info/<extension>/` 下，并由扩展目录维护自己的 `manifest.json`。Core 只提供 `extra_info/<extension>` 的安全路径边界；具体 schema 由扩展自己维护。

未来插件系统接入 lifecycle hooks 后，插件可以根据数据需求选择上述 Snapshot 相关层级。GUI 内置扩展功能也应复用这些边界，而不是扩张成 core 的 Snapshot 字段。插件自身的配置、缓存、账号状态等不属于 Snapshot 附件，后续应另行设计插件数据目录。

## 开发流程

若要为 Game-save-manager 项目做出贡献，你需要：

1. 在 GitHub 上 Fork 存储库的 `dev` 分支
2. 将 Fork 的存储库克隆到你的本地计算机
3. 为你的更改创建一个新的分支，如 `feat/webdav-essentials`
4. 对代码进行更改，将你的更改提交到你的本地分支
5. 将你的更改推送到你在 GitHub 上 Fork 的存储库
6. 创建一个 pull request，将你的更改合并到主存储库的 `dev` 分支中，注意，你总是需要以 rebase 的方式来合并代码

### 合并上游更新

在你开发一段时间之后，你可能会发现上游的代码已经更新了。为了保持你的分支与上游的代码同步，你可以使用以下命令：

```bash
git switch dev
git pull
git switch <your-branch>
git rebase dev
```

这样我们可以保持提交历史的整洁，并且避免不必要的冲突，但是如果已经有冲突了，你需要手动解决冲突，此时我们推荐使用 squash merge 的方式来合并代码。

## 使用`vue-devtools`

首先需要安装 devtools，并且正确启动

```bash
pnpm add -g @vue/devtools@next
vue-devtools
```

接下来请在项目根目录下找到`index.html`，并且在`<head>`标签中添加以下内容

```html
<script src="http://localhost:8098"></script>
```

## 编码风格

暂时没有完善的编码风格文档，如果你能帮助完成这部分文档我将不胜感激，暂时请参考其余部分代码，尽量保持简洁，且留下合适的文档

### UI 覆盖层、层级（z-index）与用户反馈

- 耗时操作使用 `src/App.vue` 的全局 Loading 覆盖层（通过 `useGlobalLoading()` 控制）。
- 右上角 toast 通知统一使用 `useNotification()`。
- 确认/输入对话框统一使用 `useFeedback()`。
- 任何弹层/覆盖层的层级（z-index）**不要散落写魔法数字**，统一使用 `src/ui/layers.ts` 中的语义化常量 `LAYER.*`，以确保通知在全局覆盖层开启时仍可见。

## 提交信息

请按照[约定式提交](https://www.conventionalcommits.org/)来编写 commit 信息，这样有助于合作以及自动化构建，你可以使用 VSCode 插件 `Conventional Commits` 来辅助编写你的提交信息

## 版本号说明

版本号的格式为`x.y.z`，其中`x`为大版本号，`y`为小版本号，`z`为修订号。其中`x`的变化大概率会导致不兼容的改动，`y`的变化可能是重要功能更新，`z`的变化只是一些小的改动，一般后两者可以自动升级。

### 更新需要做的改动

其余开发者没有必要改动版本号，只需要在更新日志中添加自己的更新内容即可。版本号会在合并进主分支时由 Maintainer 进行修改。

- 在`src-tauri\Cargo.toml`中修改版本号

## 文件夹说明

- doc: 开发文档
- public: 静态文件
- scripts: 用于 Github Action 的脚本
- src: 前端项目的源代码
  - assets: 静态资源
  - locales: 国际化资源
  - schemas: 保存数据的格式
  - 其他请参考文件夹名
- src-tauri: 后端项目的根目录
  - src: 后端项目的源代码
