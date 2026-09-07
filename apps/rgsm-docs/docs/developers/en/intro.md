---
title: Getting started
slug: /developers/en
---

[简体中文](../intro.md)

## Prerequisites

- Node.js 24 and pnpm 11
- Rust 1.97 or later
- The [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your operating system

## Run the application

```bash
git clone https://github.com/mcthesw/game-save-manager.git
cd game-save-manager
pnpm i
pnpm dev
```

`pnpm dev` starts the desktop application. Development data lives in `.rgsm-dev/app-data` within the repository.

To debug the interface and business flows in a browser:

```bash
pnpm web:dev
```

Open the local URL printed in the terminal. The default port is `5173`.

## Build

```bash
pnpm build
```

Build output lives in `target/release`, with installers in its `bundle` directory. Use `pnpm portable` for a Windows portable build.
