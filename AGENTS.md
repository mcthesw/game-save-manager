# Development Guidelines

## Structure and boundaries

- `apps/rgsm-gui/src/`: frontend; reuse `src/ui/kit`, `useNotification()`, `useFeedback()`, and `src/ui/layers.ts` (`LAYER.*`).
- `apps/rgsm-gui/src-tauri/`: desktop host, GUI hooks, and quick actions. Keep command/HTTP adapters thin; delegate business logic to services.
- `crates/rgsm-core/`: Tauri-independent business logic. Services assemble configuration, dependencies, and hooks; domain functions receive explicit inputs rather than reading global configuration.
- `locales/`: shared translations; `apps/rgsm-docs/`: documentation; `scripts/`: workspace tooling.
- Keep modules cohesive and readable; avoid files over 700 lines unless necessary.

## UI design

1. Use concrete names players understand, such as “Game root directory”; show explanations only when they help the user act.
2. Align related controls and group them through proximity and consistent spacing; add borders and dividers only when they clarify grouping.
3. Establish clear visual priority: emphasize the main content and action, keep secondary controls quiet, and avoid duplicate labels or nested boxes.
4. Reuse existing components and sizing conventions. Inspect the actual rendered UI with long content and narrow widths before submitting; controls must not stretch unrelated content, and toggles must keep surrounding layout stable.

## Implementation

- Frontend calls the Rust HTTP API, never Tauri APIs or `invoke`. Regenerate `src/api/generated/` with `pnpm --dir apps/rgsm-gui web:generate-api`; never edit generated files manually.
- Internationalize all user-facing strings: frontend `$t('key')`, backend `rust-i18n`. Add matching keys to `en_US` and `zh_SIMPLIFIED`; other locales fall back to English.

## Development and debugging

- `pnpm dev`: run the desktop app; `pnpm build`: production build; `pnpm portable`: Windows portable package.
- `pnpm web:dev`: run the real Rust HTTP host and frontend with isolated data in `.rgsm-dev/app-data`. Use this for browser-based UI and business-flow debugging.
- Use the desktop app to verify window, tray, hotkey, single-instance, and WebView-specific behavior. Open its inspector with `Ctrl+Shift+i`.
- For Windows WebView debugging, set `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` before `pnpm dev`; inspect `http://127.0.0.1:9222/json/list` for the app target.
- Platform prerequisites: <https://v2.tauri.app/start/prerequisites/>.

## Verification

For bug fixes, first reproduce the failure in a test. If that is impractical, explain why and provide the closest useful regression coverage. Manually verify affected user flows; passing automated checks does not establish visual or desktop acceptance.

Run before committing and resolve all failures and warnings:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo check --workspace
cargo test -p rgsm-core --lib
pnpm web:format
pnpm web:lint
pnpm web:typecheck
```

## Commits

- Use English Conventional Commits with emojis: `feat(backup): :sparkles: add zip64 support`. Do not include AI tool branding in branch names, commit messages, or co-author metadata.
- Keep commits cohesive and reasonably sized. Amend/rebase corrections into a clean history rather than accumulating temporary fixes.

## Documentation

- Keep root READMEs focused on users. Put app-specific usage and contributor notes under the relevant app.
- Keep documentation practical and concise. Mark OpenSpec tasks complete only after implementation and verification.
