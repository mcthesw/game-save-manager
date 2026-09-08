---
title: Debugging
---

## Interface and business flows

```bash
pnpm web:dev
```

Open the local page and use browser developer tools to inspect components, network requests, and console output. The Rust HTTP Host uses development data in `.rgsm-dev/app-data`.

After changing an HTTP contract, [regenerate the client](./architecture.md#changing-an-api) and restart the development server.

## Desktop features

```bash
pnpm dev
```

Use the desktop application to debug the tray, global hotkeys, window state, and system notifications. On Windows and Linux, `Ctrl+Shift+I` opens WebView developer tools.

On Windows, enable WebView2 remote debugging from PowerShell:

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9222'
pnpm dev
```

Find the page debugging endpoint at `http://127.0.0.1:9222/json/list`. After closing the application, clear the setting in the current terminal:

```powershell
Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
```

## Documentation

```bash
pnpm docs:dev
```

The documentation site starts at `http://localhost:3000` by default and updates as you edit pages.
