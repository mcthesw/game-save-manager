# Thin device launcher

The launcher requires Node 24. It reuses the repository's owned-process helper and starts a debug Rust binary in HTTP-only mode. It serves the built frontend, with no Vite watcher or desktop window.

Before launch:

1. Build the intended checkout with `pnpm web:build` and `cargo build --locked -p rgsm --bin rgsm`. Limit `CARGO_BUILD_JOBS` when sharing the user's computer
2. Prepare an existing data directory under `.rgsm-dev/acceptance/<task>/` using a small task-specific script. Set `settings.locale`, `quick_action.enable_sound=false`, and `quick_action.enable_notification=false`. Use test-only device IDs, saves, backup directories, and cloud storage
3. The data directory must contain `GameSaveManager.config.json`. All paths inside it remain the preparer's responsibility: the launcher confines its own writes, but cannot certify arbitrary fixture configuration

From the repository root:

```text
node .agents/skills/rgsm-gui-acceptance/scripts/device.mjs .rgsm-dev/acceptance/example/device-a review-a 5188
```

The arguments are the prepared data directory, simulated device ID, and an unused loopback port. The launcher fails on a busy port instead of adopting or stopping its owner. It copies the binary into the data directory so subsequent builds do not overwrite the running executable.

Wait for the printed ready URL before opening the GUI. Use the printed `/__acceptance` stop page for reliable shutdown, including when the launcher runs in a hidden window. Keep the process supervised; Ctrl+C in a normal terminal also requests shutdown, but force-terminating a supervisor may bypass cleanup. Closing the browser does not stop it. For an embedded packet/control page, import `startAcceptanceDevice({ dataDir, deviceId, port })`, retain the returned handle, and await `handle.stop()` during cleanup.

The helper never resets data. Restart with the same directory to retain user edits. A second launcher for the same directory is rejected by `.acceptance-running`. After an abnormal termination, inspect the original process before removing that empty lock directory; never use a saved PID as authority to kill a process.

For multiple devices, start only those required by the current scenario. Keep a local cloud fixture alive across device restarts when testing connection persistence. If the fixture itself is ephemeral, explicitly state that stopping it invalidates its join codes and remote contents.

This is a lifecycle/transport helper, not a scenario runner. Use ordinary fixture files or setup code for saves and ordinary Markdown for the reviewer. Use `node --test .agents/skills/rgsm-gui-acceptance/scripts/device.test.mjs` for the helper's path and argument checks.
