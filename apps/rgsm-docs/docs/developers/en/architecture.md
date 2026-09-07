---
title: Project structure
---

The repository uses pnpm and Cargo workspaces. Vue 3, TypeScript, and Vite power the frontend. Tauri 2 provides the desktop window and system integrations, while the Rust core library handles save management.

| Path                      | Purpose                                                              |
| ------------------------- | -------------------------------------------------------------------- |
| `apps/rgsm-gui/src`       | Pages, components, composables, and the HTTP client                  |
| `apps/rgsm-gui/src-tauri` | Desktop host, HTTP routes, tray, and hotkeys                         |
| `crates/rgsm-core`        | Backup, restore, configuration, cloud sync, and application services |
| `apps/rgsm-docs`          | User documentation and developer guides                              |
| `locales`                 | Shared frontend and backend translations                             |
| `scripts`                 | Repository build and packaging tools                                 |
| `docs`                    | Design notes and architecture decision records                       |
| `CONTEXT.md`              | Current domain terminology and conventions                           |

CLI, TUI, and FFI entry points live in `apps/rgsm-cli`, `apps/rgsm-tui`, and `crates/rgsm-ffi`.

## Following an operation

Vue page → TypeScript HTTP client → Rust route → application service → domain module

The frontend calls the HTTP API and receives events through SSE. Desktop and browser development share this interface.

- Pages and components live in `src/pages` and `src/components`; shared UI logic lives in `src/composables`
- The frontend uses Tailwind CSS, Reka UI, and project UI components
- `rgsm-core/src/services` assembles configuration, dependencies, and lifecycle hooks
- Domain modules such as `backup`, `config`, and `cloud_sync` receive explicit parameters and implement their business rules

## Changing an API

After changing the Rust HTTP contract, regenerate the client from the repository root:

```bash
pnpm --dir apps/rgsm-gui web:generate-api
```

Commit `src/api/generated` and `src/api/defaultConfig.generated.ts` together with the API change.

## UI conventions

- Use `<script setup lang="ts">` in Vue components
- Use `useNotification()` for notifications and `useFeedback()` for confirmations and prompts
- Use `LAYER` from `src/ui/layers.ts` for overlay stacking
- Use translation keys for user-facing text and update both `en_US` and `zh_SIMPLIFIED`

See [CONTEXT.md](https://github.com/mcthesw/game-save-manager/blob/dev/CONTEXT.md) for domain concepts and the [architecture decision records](https://github.com/mcthesw/game-save-manager/tree/dev/docs/adr) for design rationale.
