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

## Start from a runnable example when useful

```sh
pnpm acceptance:new cloud-journey --example
node .rgsm-dev/acceptance/cloud-journey/run.mjs
```

Currently verified on Windows, with Node 24, installed pnpm dependencies and the repository's
Rust build prerequisites. This builds one HTTP-only host binary and one web bundle, starts
two isolated hosts with a shared local Fs cloud, and prints each device's GUI and text-panel
links. Ports are assigned by the OS; existing dev servers are not adopted or stopped.
No browser or desktop window opens automatically. The panel reads/writes only its fixed
artificial save file, never arbitrary paths. It is local tooling, not a product UI.

Edit the generated `run.mjs` directly: remove B, replace the seed, or change initial bytes.
`ACCEPTANCE.md` is a suggested human outline, not machine-parsed input. Store actual state
in fixture code and sample files, not Markdown/YAML encodings. Keep sample assets small and
synthetic. The example intentionally leaves library creation, snapshots, transfers and
restore for real RGSM interactions; it makes no WebDAV/S3 or desktop-shell claims.

`apps/rgsm-gui/scripts/acceptance/` contains the app-specific build/preview/text helpers.
The example directly imports `e2e/support/cloud-fixture.ts`; readiness uses the same
`scripts/wait-http-host.ts` helper as E2E, and process ownership uses `e2e/support/process.ts`.
Other E2E fixture modules may require a compatible runner for extensionless TS imports:
do not assume all E2E code runs in plain Node. No scenario registry or generic adapter layer
is needed to write another packet.

Each launch records the actual revision, dirty files, artifact and environment under
`evidence/runtime.json`. Creation metadata alone is not execution evidence. Restart preserves
data but rebuilds incrementally and prints new URLs/tokens; reload through the new links.
Use either text panel's stop button or Ctrl+C. Generated configs/logs remain private local
data and can contain credentials; share only redacted evidence.

Run lightweight scaffold/readiness tests with `pnpm acceptance:test`. The opt-in
`pnpm acceptance:test:example` additionally builds and exercises the generated example
against the real backend with headless Chromium (Playwright browser installation required).
Neither command records a human acceptance result.
