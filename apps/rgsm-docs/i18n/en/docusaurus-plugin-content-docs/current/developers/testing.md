---
title: Testing
---

Run these commands from the repository root. Start with tests relevant to your change, then complete the checks before committing.

## Rust

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo check --workspace
cargo test -p rgsm-core --lib
```

Configuration upgrades and cloud sync also have integration tests:

```bash
cargo test -p rgsm-core --test config_upgrade_compatibility
cargo test -p rgsm-core --test cloud_sync_v2_fs
```

## Frontend

```bash
pnpm web:format
pnpm web:lint
pnpm web:typecheck
pnpm web:test
```

Run `pnpm web:format:fix` to fix formatting, then review the diff.

## End-to-end tests

Install Chromium before the first run:

```bash
pnpm --dir apps/rgsm-gui exec playwright install chromium
pnpm web:e2e local-main-path.spec.ts
```

Tests use the real Rust HTTP Host, temporary saves, and local Fs cloud storage. Free port `5173` before starting.

`pnpm web:e2e` runs the full browser suite; `pnpm web:e2e:desktop` runs desktop scenarios. See the [E2E notes](https://github.com/mcthesw/game-save-manager/blob/dev/apps/rgsm-gui/e2e/README.md) for platform dependencies and scenarios.

## Documentation

```bash
pnpm docs:build
```

The build checks documentation links. Check CI results on your PR after pushing.
