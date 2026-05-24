# RGSM TUI

`rgsm-tui` is an experimental terminal interface for Game Save Manager. It is not recommended for anyone to use.

It currently exists to explore keyboard-first backup, restore, cloud sync, and Ludusavi import workflows.

## Run

From the workspace root:

```bash
cargo run -p rgsm-tui
```

By default, the TUI uses its own profile directory. It does not read or write the GUI profile unless you explicitly import it.

Use an explicit TUI profile directory when testing or working with a portable setup:

```bash
cargo run -p rgsm-tui -- --data-dir <path>
```

You can also set `RGSM_TUI_DATA_DIR=<path>`.

Import an existing GUI profile into the TUI profile:

```bash
cargo run -p rgsm-tui -- --import-gui-config <gui-profile-or-config-path>
```

The import is one-way. The GUI profile is read-only; games, devices, selected backup settings, cloud settings, VN scan directories, and missing backup files are copied into the TUI profile. GUI-only settings such as tray actions, appearance, homepage, and favorites are left out.

## Keys

- `1`-`6`: switch screens
- `Tab` / `Shift+Tab`: switch panes
- `Arrow keys` or `hjkl`: move selection
- `Enter`: run the main action for the current screen
- `?`: show help
- `q`: quit

Screen-specific actions are shown in the footer. The Ludusavi screen uses `f` to switch between locally detected games and the full manifest list.

## Contributor Notes

- Keep terminal-only state in `rgsm-tui.settings.json`.
- Do not point the TUI profile at the GUI profile. Use the import flow instead.
- Keep business logic in `rgsm-core`; the TUI should only coordinate input, rendering, and calls into shared services.
- Keep TUI user-facing text in `apps/rgsm-tui/locales/en_US.json` and `apps/rgsm-tui/locales/zh_SIMPLIFIED.json`.
