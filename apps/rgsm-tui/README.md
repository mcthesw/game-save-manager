# RGSM TUI

`rgsm-tui` is an experimental terminal interface for Game Save Manager. It is not recommended for anyone to use.

It currently exists to explore keyboard-first backup, restore, cloud sync, and Ludusavi import workflows.

## Run

From the workspace root:

```bash
cargo run -p rgsm-tui
```

Use an explicit data directory when testing or working with a portable setup:

```bash
cargo run -p rgsm-tui -- --data-dir <path>
```

You can also set `RGSM_DATA_DIR=<path>`.

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
- Keep business logic in `rgsm-core`; the TUI should only coordinate input, rendering, and calls into shared services.
- Keep new user-facing text in `locales/en_US.json` and `locales/zh_SIMPLIFIED.json`.
