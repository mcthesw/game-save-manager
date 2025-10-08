# AI Agent Development Guidelines for Game Save Manager

This document provides guidance for an AI agent working on this repository. Your goal is to understand the project structure, conventions, and workflows to contribute effectively.

## Project Overview

This is a cross-platform desktop application for managing game saves, built with Tauri (Rust backend) and Nuxt 3 (Vue 3 frontend). It features local backups, cloud synchronization (WebDAV/S3), and quick actions via hotkeys and a system tray menu.

This project depends on the following softwares:

### Tauri deps

The code block below shows how to install tauri's deps in Debian. For more information, see <https://v2.tauri.app/start/prerequisites/>.

```bash
# For debian
sudo apt install libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libxdo-dev \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev

```

## Development Commands

- `pnpm install`: Install all dependencies and run `nuxt prepare`.
- `pnpm dev`: Run the full application in development mode with hot-reloading.
- `pnpm build`: Build the application for production.
- `pnpm web:dev`: Run the frontend only for UI-focused work.
- `pnpm portable`: Create a portable build.

## Project Structure & Module Organization

The application is divided into a frontend and a backend.

- **Frontend (`src/`)**: A Nuxt 3 application.
  - `pages/`: Routed Vue components.
  - `components/`: Reusable Vue components.
  - `composables/`: Shared state and logic using Vue Composition API.
  - `assets/`: Static assets like CSS and images.
  - `locales/`: i18n translation files (JSON).

- **Backend (`src-tauri/`)**: A Rust-based Tauri application.
  - `src/main.rs`: Application entry point.
  - `src/lib.rs`: Main library, defines Tauri commands.
  - `src/ipc_handler.rs`: Handles Inter-Process Communication (IPC) commands.
  - `src/backup/`: Logic for creating and restoring game save backups.
  - `src/cloud_sync/`: Logic for WebDAV and S3 synchronization.
  - `src/config/`: Manages `GameSaveManager.config.json`.
  - `src/quick_actions/`: Implements hotkeys, tray menu, and timers.
  - Any IPC commands should be placed in `ipc_handler.rs`.

- **Contracts (`src/bindings.ts`)**: This auto-generated file contains TypeScript definitions for all Rust `#[tauri::command]` functions. It is the primary contract between the frontend and backend. **Never edit it manually.**

## Coding Style & Naming Conventions

- **Frontend (Vue/TypeScript)**:
  - Use `<script setup lang="ts">` for all Vue components.
  - Components: `PascalCase` (e.g., `GameList.vue`).
  - Composables: `camelCase` with a `use` prefix (e.g., `useConfig.ts`).
  - Use Element Plus for UI consistency.
  - Never use tauri's `invoke<T>(cmd: string, args?: InvokeArgs, options?: InvokeOptions): Promise<T>`, you can use `pnpm dev` to launch app so that `src/bindings.ts` will be updated.

- **Backend (Rust)**:
  - Modules/Files: `snake_case` (e.g., `cloud_sync.rs`).
  - Types/Structs: `PascalCase`.
  - Functions/Variables: `snake_case`.
  - Use `Result` and `thiserror`/`anyhow` for robust error handling. (prefer `thiserror` in internal modules)
  - Always run `cargo clippy` and clear all warns before commit.

## Commit & Pull Request Guidelines

Follow the Conventional Commit specification with emojis. The format is `type(scope): :emoji: summary`.

- **Example**: `feat(backup): :sparkles: add support for zip64 archives`
- Keep commits small and focused on a single logical change.
- Pull requests must include a clear description, testing steps, and screenshots for any UI changes.
- Wait for CI checks to pass before requesting a review.

## Localization (i18n)

All user-facing strings must be internationalized.

- **Frontend**: Use the `$t('key')` function from `vue-i18n`. Strings are in `locales/*.json`.
- **Backend**: Use the `rust-i18n` crate.
- To add a new string, add the key to `locales/en_US.json` and its translation to other locale files. (`en_US` and `zh_SIMPLIFIED` are the tier 1 locales)

## Testing Guidelines

The project currently relies on manual testing. Before submitting a pull request, verify that core features work as expected:

- Backup and restore operations.
- Cloud synchronization with a test account.
- Hotkey and system tray functionality.
- Settings are saved and loaded correctly after restarting the app.
- Always run clippy(and other linters) before committing.
