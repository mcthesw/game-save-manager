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
- WebStorm
- RustRover

### 安装依赖

`pnpm i`

### 编译与开发

请参考`package.json`来了解指令
- `pnpm tauri:dev` 开发模式，一边预览一边开发
- `pnpm tauri:build` 编译打包，输出会存放在`src-tauri/target`

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
6. 创建一个 pull request，将你的更改合并到主存储库的 `dev` 分支中

## 编码风格

暂时没有完善的编码风格文档，如果你能帮助完成这部分文档我将不胜感激，暂时请参考其余部分代码，尽量保持简洁，且留下合适的文档

## 提交信息

请按照[约定式提交](https://www.conventionalcommits.org/)来编写 commit 信息，这样有助于合作以及自动化构建，你可以使用 VSCode 插件 `Conventional Commits` 来辅助编写你的提交信息

## 版本号说明

版本号的格式为`x.y.z`，其中`x`为大版本号，`y`为小版本号，`z`为修订号。其中`x`的变化大概率会导致不兼容的改动，`y`的变化可能是重要功能更新，`z`的变化只是一些小的改动，一般后两者可以自动升级。

### 更新需要做的改动

其余开发者没有必要改动版本号，只需要在更新日志中添加自己的更新内容即可。版本号会在合并进主分支时由 Maintainer 进行修改。

- 在`package.json`中更新版本号
- 在`src/schemas/saveTypes.ts`中更新版本号
- 在`src-tauri/src/config.rs:default_config`中更新版本号

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