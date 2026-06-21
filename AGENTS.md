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

- `pnpm install`: Install workspace dependencies and run `nuxt prepare` for `apps/rgsm-gui`.
- `pnpm dev`: Run the Tauri GUI app in development mode.
- `pnpm build`: Build the Tauri GUI app for production.
- `pnpm web:dev`: Run the Nuxt frontend only.
- `pnpm web:lint`: Run the frontend ESLint checks.
- `pnpm web:typecheck`: Run the frontend typecheck.
- `pnpm portable`: Create a Windows portable build from the workspace root.

## Tauri UI Debugging

- Use `pnpm dev` when validating frontend behavior that depends on Tauri IPC. `pnpm web:dev` only serves the Nuxt frontend in a normal browser context and does not provide the Tauri IPC bridge.
- On Windows, agents can expose the Tauri WebView2 DevTools Protocol endpoint without changing repo config:

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9222'
pnpm dev
```

- After the Tauri window is running, inspect `http://127.0.0.1:9222/json/list` and attach to the page target whose URL is the configured `devUrl` (usually `http://localhost:3000/`). This validates the actual Tauri WebView with IPC available, not a separate browser tab.
- For quick manual inspection inside the Tauri window, use the WebView inspector (`Ctrl+Shift+i` on Windows/Linux) or call `open_devtools()` in debug-only setup code.
- Do not claim a GUI feature is verified from `pnpm web:dev` alone if the feature depends on Tauri commands, events, app config, or WebView-specific behavior.

## Project Structure & Module Organization

The repository is split into workspace apps and crates.

- **GUI app (`apps/rgsm-gui/`)**: Nuxt 3 frontend plus the Tauri host.
  - `src/`: Routed Vue UI (`pages/`, `components/`, `composables/`, `assets/`).
  - `src/bindings.ts`: Auto-generated TypeScript bindings for Rust `#[tauri::command]` APIs. **Never edit it manually.**
  - `src-tauri/src/lib.rs`: Tauri bootstrap and state wiring.
  - `src-tauri/src/ipc_handler.rs`: **Thin export layer only.** Commands should stay 1-3 lines and delegate to services/domain modules.
  - `src-tauri/src/hooks/`: GUI-only hooks such as notifications and scheduler sync.
  - `src-tauri/src/quick_actions/`: GUI-only tray, hotkey, and timer integrations.

- **Core library (`crates/rgsm-core/`)**: Pure Rust business logic with no Tauri dependency.
  - `backup/`, `config/`, `cloud_sync/`: Domain modules.
  - `services/`: Orchestration entry points used by IPC/CLI/FFI layers.
  - `hooks/`: `LifecycleHook`, contexts, DI traits, and `HookPipeline`.
  - `path_resolver.rs`, `app_dirs.rs`, `device.rs`, etc.: Shared infrastructure.

- **Future integration crates**
  - `apps/rgsm-cli/`: CLI app placeholder.
  - `crates/rgsm-ffi/`: FFI crate placeholder.

- **Shared assets**
  - `locales/`: Shared i18n files. Tier 1 locales are `en_US` and `zh_SIMPLIFIED`.
  - `scripts/`: Repo-level helper scripts such as portable packaging.

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

## Pre-commit Checks

Run all of the following before each commit to catch issues early. They can be run in a single command chain:

```bash
# Rust: format, lint, and test
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo check --workspace
cargo test -p rgsm-core --lib

# Frontend: lint and typecheck
pnpm web:format
pnpm web:lint
pnpm web:typecheck
```

CI will verify these checks. Fix any issues before committing.

## Commit & Pull Request Guidelines

Follow the Conventional Commit specification with emojis. The format is `type(scope): :emoji: summary`.

- **Example**: `feat(backup): :sparkles: add support for zip64 archives`
- Do not add AI agent/tool branding or names (for example Codex) to branch names, pull request titles/descriptions, commit messages, or co-author metadata.
- Keep commits small and focused on a single logical change.
- Split broad work into cohesive, reviewable commits by behavior or architectural layer. Do not hide a large feature, bug fix, and documentation cleanup in one giant commit.
- When a branch has not been merged yet, prefer rewriting local branch history into a clean commit series over adding follow-up fix commits for issues introduced by the same change.
- Pull requests must include a clear description, testing steps, and screenshots for any UI changes.
- Wait for CI checks to pass before requesting a review.

## Documentation Guidelines

- Treat the root README files as user-facing project entry points. Do not add implementation notes, internal planning statements, or self-referential limitations there unless they directly help users.
- Put app-specific usage and contributor notes under that app's directory, and keep them concise, practical, and audience-focused.
- Do not mark OpenSpec tasks complete unless the behavior is implemented and verified. If reality contradicts the task list, fix the implementation or the task state before presenting the change as done.

## Localization (i18n)

All user-facing strings must be internationalized.

- **Frontend**: Use the `$t('key')` function from `vue-i18n`. Strings are in `locales/*.json`.
- **Backend**: Use the `rust-i18n` crate.
- To add a new string, add the key to `locales/en_US.json` and its translation to other locale files. (`en_US` and `zh_SIMPLIFIED` are the tier 1 locales)
- **Tier 1 locale files must stay in sync**: when adding new keys, you must add the text for `en_US` and `zh_SIMPLIFIED`. Tier 2 locales (`fr`, `ko`, `ta`, `uk`) automatically fall back to English and do not need manual placeholder entries for new keys.

## Testing Guidelines

The project has both automated tests and manual verification. Before submitting a pull request:

- Bug-fix workflow (TDD-first): for bug fixes, write a test that reproduces the bug first, confirm it fails, then implement the fix and make the test pass.
  - If TDD-first is not practical (e.g. third-party outage, platform-only behavior that cannot be reliably reproduced in CI, or urgent hotfix constraints), clearly document the reason and provide the closest possible automated regression coverage.

- Run automated checks:
  - `cargo check --workspace`
  - `cargo test -p rgsm-core --lib` (or broader `cargo test` when related)
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `pnpm web:typecheck`
- Ensure all checks pass without warnings.
- Then verify core features manually:

- Backup and restore operations.
- Cloud synchronization with a test account.
- Hotkey and system tray functionality.
- Settings are saved and loaded correctly after restarting the app.
