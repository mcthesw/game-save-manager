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

## 架构

该软件分为两个主要部分：

- 前端负责用户界面和交互。它使用 TypeScript 和 Vue3 编写
  - 使用 Element Plus 组件库
  - 使用 pinia 进行状态管理
  - 使用 vue-router 作为前端路由
  - 使用 vue-i18n 进行国际化
- 后端负责管理游戏存档文件。它使用 Rust 编写
  - 使用 opendal 来访问云存储
  - 使用 serde 来序列化和反序列化数据
  - 使用 thiserror 和 anyhow 进行错误处理

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
