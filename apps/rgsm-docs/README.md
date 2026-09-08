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

用户指南和开发者指南位于 `docs/`，更新日志位于 `blog/`，英文翻译位于 `i18n/en/`。架构决策和内部设计文档保留在仓库根目录的 `docs/`。

使用 `pnpm --dir apps/rgsm-docs dev --locale en` 预览英文站点。生产构建会同时生成中英文站点。

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

User and developer guides live in `docs/`, release posts in `blog/`, and English translations in `i18n/en/`. Architecture decisions and internal design documents remain in the repository root `docs/`.

Use `pnpm --dir apps/rgsm-docs dev --locale en` to preview the English site. Production builds include both languages.
