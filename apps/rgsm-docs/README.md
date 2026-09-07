# 用户文档

本目录包含[游戏存档管理器](https://github.com/mcthesw/game-save-manager)的用户文档站点，使用 [Docusaurus](https://docusaurus.io/) 构建。

## 本地开发

在仓库根目录安装依赖并启动文档站点：

```bash
pnpm install
pnpm docs:dev
```

生产构建：

```bash
pnpm docs:build
```

用户指南位于 `docs/`，更新日志位于 `blog/`。架构决策和内部设计文档保留在仓库根目录的 `docs/`。

## 截图

在已准备好 GUI 开发环境和 Playwright Chromium 的工作区，执行：

```bash
pnpm --dir apps/rgsm-gui exec playwright test --config scripts/docs-screenshots.config.ts
```

脚本使用真实 Rust HTTP Host、临时演示数据和本地 Fs 云端，生成 `static/img/guide/` 下的中文截图，不读取玩家数据。正文通过 `Screenshot` 组件统一呈现细边框、圆角和轻阴影；历史更新日志保留当时的界面。

## Cloudflare Pages 发布

文档站保留 `https://help.sworld.club`，由主仓库的 `apps/rgsm-docs` 构建。Pages Git 集成使用以下参数：

| 项目 | 值 |
| --- | --- |
| Git 仓库 / 生产分支 | `mcthesw/game-save-manager` / `dev` |
| 构建根目录 | 仓库根目录 |
| 构建命令 | `pnpm docs:build` |
| 构建产物目录 | `apps/rgsm-docs/build` |
| 环境变量 | `NODE_VERSION=24`、`PNPM_VERSION=11.1.2` |
| 构建监视路径 | `apps/rgsm-docs/**`、`package.json`、`pnpm-lock.yaml`、`pnpm-workspace.yaml` |

迁移时先在新 Pages 项目的预览地址验证首页、旧文档路径、更新日志和图片，再迁移自定义域名。`help.sworld.club` 的 DNS 由 `sworld-infra` 的 OpenTofu 配置维护，域名切换时同步修改对应配置。旧 Pages 项目先保留，必要时可以将域名切回。

文档构建检查由 `.github/workflows/docs.yml` 执行；该工作流不修改 Cloudflare，也不发布网站。

---

# User documentation

This directory contains the [Game Save Manager](https://github.com/mcthesw/game-save-manager) user documentation site, built with [Docusaurus](https://docusaurus.io/).

## Local development

Install dependencies and start the documentation site from the repository root:

```bash
pnpm install
pnpm docs:dev
```

Create a production build:

```bash
pnpm docs:build
```

User guides live in `docs/`, and release posts live in `blog/`. Architecture decisions and internal design documents remain in the repository root `docs/`.
