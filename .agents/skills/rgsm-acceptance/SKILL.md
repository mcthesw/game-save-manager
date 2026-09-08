---
name: rgsm-acceptance
description: Offer and prepare optional, change-specific human acceptance for RGSM after implementation, or when asked for an acceptance environment for recovery, multi-device use or another changed journey
---

# Optional RGSM acceptance

Offer useful human acceptance, then prepare a small usable packet if wanted.
This is optional, not a PR gate, a replacement for tests or a fixed scenario catalog.

## Offer only when useful

Inspect the change and existing verification. If human judgment adds value, briefly offer
the journey, environment requirements and approximate effort; otherwise skip the offer.
Wait for opt-in before preparing or starting an environment. A direct request already
opts in; do not ask again. Overlap with automated coverage is fine when useful.

Match evidence to the environment: dev frontend, built web and packaged desktop are distinct.
Tray/window behavior needs the desktop shell. Windows browser use is not Linux proof;
this scaffold does not provision Linux. Mark unavailable execution as not run.
Do not install VMs, use remote hosts, open windows or use personal data without authorization.

## Prepare the packet

Read `scripts/acceptance/README.md` for commands, lifecycle helpers and fixture locations.
Use `pnpm acceptance:new <name> [platform]` when it fits. Add `--example` on Windows for
a runnable A/B + local Fs cloud + text-file starting point; freely trim or replace it.
Generated packets are local ignored output, not permanent test cases.

Adapt `ACCEPTANCE.md` and `run.mjs` to the change. Markdown describes initial state,
numbered actions and observable results; its headings are suggestions, not a schema.
Trusted fixture code and small synthetic assets create actual state, not a JSON/YAML
action language. A control page is optional. Use at most five focused scenarios by default.

Use isolated app data, artificial saves and local test cloud storage. Reuse fixture and
process helpers; do not mirror product logic. Automate prerequisites, leaving the behavior
being accepted for the human to exercise. Keep builds/process counts small and reuse valid
builds. No OS notifications or automatic browser/window launch. Stop/resume must preserve
reviewer changes; a fresh baseline is a separately named packet.

Before handoff, smoke-test entry points, initial state, readiness, shutdown and restart
with modified test files. Record the actual source/artifact, environment and dirty files;
packet creation metadata and a runnable example alone do not prove the changed journey.

## Hand off and follow up

Give links, short paired actions/results, stop/resume instructions and coverage boundaries.
The reviewer should not need to edit app configuration. Ask for the step number, actual
outcome and useful screenshots, not logs containing tokens. Separate setup checks, human
results and not-run items; only the human's report establishes human acceptance.

Follow up within the implementation's scope. Promote helpers, fixtures or regression tests
into tracked source when reuse justifies it; do not permanently add every generated packet.
