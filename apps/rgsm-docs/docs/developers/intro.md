---
title: 开始开发
slug: /developers
---

## 准备环境

- Node.js 24、pnpm 11
- Rust 1.97 或更新版本
- 当前系统的 [Tauri 2 开发依赖](https://v2.tauri.app/start/prerequisites/)

## 运行项目

```bash
git clone https://github.com/mcthesw/game-save-manager.git
cd game-save-manager
pnpm i
pnpm dev
```

`pnpm dev` 启动桌面程序。开发数据保存在仓库的 `.rgsm-dev/app-data`。

在浏览器中调试界面和业务流程：

```bash
pnpm web:dev
```

打开终端显示的本地地址，默认端口为 `5173`。

## 构建

```bash
pnpm build
```

产物位于 `target/release`，安装包位于其 `bundle` 目录。Windows 便携版使用 `pnpm portable` 构建。
