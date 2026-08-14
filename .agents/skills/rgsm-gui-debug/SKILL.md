---
name: rgsm-gui-debug
description: Debug the RGSM frontend against the real Rust backend in a normal browser, without Tauri IPC or WebView tooling.
---

# RGSM GUI Debug

Use this workflow for frontend and business-flow debugging:

1. From the repository root, start `pnpm web:dev` as a supervised long-running process.
2. Wait for Vite to report `http://localhost:5173/` as ready.
3. Open that URL with the browser tool; inspect the accessibility tree before interacting.
4. Exercise the real UI and verify the changed behavior in the browser.
5. Close the browser tab and stop the supervised process when finished.

`pnpm web:dev` starts the real Rust HTTP Host and Vite together. It regenerates the OpenAPI TypeScript client and uses isolated, persistent data under `.rgsm-dev/app-data`; do not create mocks or point it at the user's production data.

Use `pnpm dev` only when the behavior belongs to the desktop shell itself, such as window state, tray, global hotkeys, or single-instance handling. Follow `AGENTS.md` for Tauri WebView inspection in that case.
