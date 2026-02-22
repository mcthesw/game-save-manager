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
  librsvg2-dev \
  libasound2-dev \
  pkg-config

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
  - `src/ipc_handler.rs`: **Thin export layer only.** This file should only contain `#[tauri::command]` function signatures that delegate to other modules. Do not put business logic here - keep commands simple (1-3 lines) that just call functions from domain modules and handle error conversion. Complex logic belongs in dedicated modules like `backup/`, `config/`, `path_resolver.rs`, etc.
    - For cloud sync commands, IPC should call the cloud-sync facade and must not construct/use OpenDAL `Operator` directly.
  - `src/backup/`: Logic for creating and restoring game save backups.
  - `src/cloud_sync/`: Logic for WebDAV and S3 synchronization.
    - `backend.rs`: Backend config and OpenDAL operator creation (with retry policy).
    - `transfer.rs`: Unified streaming transfer abstraction and hook extension points.
    - `utils.rs`: Cloud sync workflows (full upload/download, metadata sync).
    - `facade.rs`: Domain entry points used by IPC layer.
  - `src/config/`: Manages `GameSaveManager.config.json`.
  - `src/quick_actions/`: Implements hotkeys, tray menu, and timers.
  - `src/path_resolver.rs`: Path variable resolution and filesystem checks.
  - Any IPC commands should be placed in `ipc_handler.rs`, but their implementation should be in domain modules.

- **Contracts (`src/bindings.ts`)**: This auto-generated file contains TypeScript definitions for all Rust `#[tauri::command]` functions. It is the primary contract between the frontend and backend. **Never edit it manually.**

## Coding Style & Naming Conventions

- **Frontend (Vue/TypeScript)**:
  - Use `<script setup lang="ts">` for all Vue components.
  - Components: `PascalCase` (e.g., `GameList.vue`).
  - Composables: `camelCase` with a `use` prefix (e.g., `useConfig.ts`).
  - Use Element Plus for UI consistency.
  - **User feedback (toast/confirm/prompt)**:
    - Toast notifications: use `useNotification()` (do not call `ElNotification` directly in pages/components).
    - Confirm/prompt dialogs: use `useFeedback()` (do not call `ElMessageBox` directly in pages/components).
  - **Overlay & z-index**:
    - Do not introduce scattered z-index magic numbers.
    - Use `src/ui/layers.ts` (`LAYER.*`) for any overlay/notification/dialog layering decisions.
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

The project has both automated tests and manual verification. Before submitting a pull request:

- Run automated checks:
  - `cargo check`
  - `cargo test --lib` (or `cargo test` when related)
  - `cargo clippy --all-targets --all-features`
- Ensure all checks pass without warnings.
- Then verify core features manually:

- Backup and restore operations.
- Cloud synchronization with a test account.
- Hotkey and system tray functionality.
- Settings are saved and loaded correctly after restarting the app.
