# Optional human acceptance packets

Node 24 and Git are sufficient to create a packet. No application starts implicitly:

```sh
pnpm acceptance:new save-restore
```

This creates `.rgsm-dev/acceptance/save-restore/` with an editable `ACCEPTANCE.md`,
`run.mjs`, and preparation metadata. The author adapts the packet to the change, then
checks its setup before offering it to a reviewer. The generated starter is deliberately
labelled as a scaffold, not an RGSM environment or a passing acceptance test.

Run the adapted packet from its repository checkout:

```sh
node .rgsm-dev/acceptance/save-restore/run.mjs
```

`withSession(entryUrl, callback)` provides isolated `dataDir`, `logsDir`, `evidenceDir`,
and these small building blocks:

- `prepare(seed)`: seed once; a successful marker preserves subsequent reviewer edits
- `start(command, args, options)`: launch a long-running service without a shell;
  output goes to local logs; process cleanup reuses the existing E2E process helper
- `wait()`: keep the launcher alive until Ctrl+C or a child failure
- `stop()`: request shutdown, useful in a setup smoke test
- `signal`: cancellation for setup/readiness; pass it to awaited I/O and use bounded timeouts
- `defer(close)`: register an owned server/resource for reverse-order cleanup

`start()` treats even a successful service exit as unexpected. Use an ordinary awaited
operation for one-shot preparation, for example:

```js
await promisify(execFile)(command, args, {
  windowsHide: true,
  signal: session.signal,
});
```

Ctrl+C or a service failure aborts `signal`, including before `wait()`. Setup must cooperate
with cancellation; JavaScript that ignores the signal cannot be forcibly interrupted safely.
The launcher waits for setup to unwind before closing resources, so it cannot continue
writing in the background after cleanup. Failed setup retains partial files for diagnosis;
fix it idempotently or create a fresh named packet, never silently reseed existing progress.

Child services must have explicit readiness checks and bind to loopback. Never pass a
normal application profile to them. Use explicit isolated app-data and device IDs for
RGSM, and disable notification/sound integrations in prepared configuration. Do not log
tokens in acceptance results. The helper does not sandbox trusted fixture code.

Stop and restart preserve data. A new packet name creates a fresh baseline; there is no
implicit reset or recursive cleanup command. Concurrent launch is rejected. After a hard
crash, confirm the old services have stopped before manually removing the empty `.running`
directory. Cleanup never kills processes by reading a PID saved in a previous session.

Packets are local, ignored output, not deployment bundles. If copying a packet, its
`session.json` is also required; do not copy generated credentials or machine-specific data.
Platform recording does not establish compatibility. This tool does not provision other
machines or provide Linux acceptance support.

## Reuse before adding adapters

Existing fixture builders live in `apps/rgsm-gui/e2e/support/`: `released-upgrade.ts`,
`local-fixture.ts`, and `cloud-fixture.ts`. HTTP host readiness and browser runtime wiring
are in `rgsm-instance.ts`; process ownership is in `process.ts`. These TypeScript modules
have different runtime dependencies: importing a fixture that transitively imports
extensionless TS modules needs a compatible runner. Do not assume plain Node can run all
E2E modules merely because it can strip TypeScript types.

Prefer a narrowly shared helper when repeated acceptance needs justify extracting one.
Do not duplicate RGSM's cloud model, add a scenario DSL, or refactor all E2E support for
an initial packet. An HTML control page, multiple devices, Playwright, and desktop launch
are optional choices for the particular request, not requirements of the scaffold.

Linux file/permission behavior needs a Linux backend; tray and window behavior additionally
need a real desktop session. An HTTP-only browser session does not establish either desktop
compatibility or packaged-app startup. Existing `production-startup.spec.ts` covers built web
startup separately. Do not install VMs or operate remote machines merely to fill a coverage gap.

Run the lightweight scaffold/lifecycle tests with `pnpm acceptance:test`.
