---
name: rgsm-gui-acceptance
description: Prepare change-specific RGSM GUI acceptance environments and short manual scenarios when the user wants to try a change locally. Uses isolated saves and the real Rust backend; not a mandatory gate for every PR or a replacement for automated tests.
---

# RGSM GUI acceptance

Deliver working GUI links, a small set of numbered scenarios, and a clear way to stop the owned services. The user performs acceptance; preparing fixtures or passing an automated smoke is not their approval.

## Select the scenarios

Read the relevant diff, current API, locale labels, and existing tests. Pin the tested revision, including any local combination of unmerged PRs. Preserve the user's checkout and other running environments.

Aim for at most five short scenarios unless the user asks for more. Cover the changed behavior and its next useful action: for example, retaining a local game after joining a library is incomplete coverage unless the user can subsequently enable its sync and transfer a snapshot.

Use real files or fixture generators for save state. Markdown describes that state and references its files; it does not serialize archives, registry data, or an execution graph. Start from [the scenario card](assets/scenario.md), adapting or removing fields that do not help this review.

## Prepare an isolated packet

Put generated scenario code, data, logs, and evidence under `.rgsm-dev/acceptance/<task>/`. Reuse helpers under `apps/rgsm-gui/e2e/support/`, especially `local-fixture.ts`, `cloud-fixture.ts`, and `process.ts`, rather than copying product schemas. Use current APIs for subsequent mutations. Never seed real save paths or cloud credentials.

Choose the lightest environment that exercises the behavior:

- For frontend/business flows, build the frontend once and use the real HTTP-only host. [The thin device launcher](scripts/device.mjs) serves the built GUI, injects runtime authentication, and owns one isolated host. It does not seed data, run scenarios, or rebuild on startup
- For tray, hotkeys, window lifecycle, WebView, or installer changes, request or prepare the actual desktop environment instead; browser success cannot prove those behaviors
- For cloud flows, use isolated local test storage. A mock WebDAV transport is suitable for join/metadata UX but does not prove remote provider compatibility

Read [launcher usage](references/environment.md) before using the helper. Keep test-specific setup and controls local to the packet. Do not grow a YAML DSL, scenario registry, generic fault engine, or permanent fixture catalog from one acceptance request.

Do not open visible windows automatically. Respect existing ports and services. Limit concurrent builds/hosts, reuse valid build artifacts, and disable notifications and sounds in test configs. Keep cloud credentials, join codes, and API tokens out of console output and screenshots.

## Verify before handing over

Exercise the numbered operations in an owned browser context. Inspect current accessibility labels before choosing selectors. Wait for actual responses and refreshed state, not incidental sleeps. Check real save contents and effective configuration, not only success notifications.

For partial failures, show which user-visible outcome succeeded and which failed. A real isolated fault is preferable when safe and repeatable; disclose any mocked failure. Record unexpected behavior instead of hiding it behind an altered scenario.

Use a separate smoke dataset from the user dataset. Do not reset or modify an environment the user has started accepting without permission. Verify stop/restart behavior, distinguishing a device restart from recreating an ephemeral cloud fixture.

## Hand over and close

Provide the entry links, numbered actions and expected results, tested revision, and any relevant known limits. Distinguish automated smoke, pending human acceptance, and untested platforms. A small landing page with state inspection is useful when file effects are otherwise hard to see, but is optional.

Keep the environment available until the user finishes or asks to stop. Closing a browser tab does not stop the host. Report exactly which owned services were stopped; only delete task data when requested, after validating the targets. Do not merge PRs or close issues merely because acceptance passed.
