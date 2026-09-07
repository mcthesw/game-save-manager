---
name: rgsm-acceptance
description: Offer and prepare optional, change-specific human acceptance for RGSM after implementation, or when asked for an acceptance environment such as Linux compatibility, upgrade recovery, or multi-device use
---

# Optional RGSM acceptance

Help the implementer decide whether human use can reveal something meaningful beyond
the existing automatic checks, then prepare a small usable acceptance packet if wanted.
This is not a mandatory PR gate, a replacement for tests, or a fixed scenario catalog.

## Offer only when useful

Inspect the actual change and existing verification. When human acceptance adds value,
offer the proposed journey, environment requirements and approximate effort briefly.
Wait for the implementer to opt in before building or starting the acceptance environment.
A direct request to prepare an acceptance environment already opts in; do not ask again.
If existing evidence is sufficient, skip the offer rather than manufacturing work.

Match the environment to the claim. Linux path/permission behavior requires a Linux
backend, not a Linux-labelled browser on Windows. Window, tray and file-picker behavior
needs the actual desktop shell. Distinguish dev frontend, built web output and packaged app.
For an unavailable platform, offer portable preparation materials and mark execution as
not run. Do not install a VM, use a remote host, open desktop windows or use personal data
without the corresponding authorization.

## Prepare the packet

Read `scripts/acceptance/README.md` from the repository root for the scaffolding command,
lifecycle helpers and existing fixture locations. Use `pnpm acceptance:new <name> [platform]`
when its structure fits. It produces local ignored output, not a permanent test case.

Adapt `ACCEPTANCE.md` and `run.mjs` to the changed journey. Markdown explains player-visible
state and results; trusted fixture code and small sample assets create actual state.
Do not invent a JSON/YAML action language. A browser control page is useful for some
multi-device tasks, but unnecessary for others. Prepare at most five focused scenarios
by default; fewer is better when they cover the relevant behavior.

Use artificial saves, isolated app data and local test cloud storage. Reuse existing
fixtures and process helpers where practical; do not mirror product logic in the scaffold.
Prepare prerequisites automatically, but leave the behavior being accepted for the human
to exercise. Keep process counts and builds small, reuse valid builds, and avoid OS
notifications or automatic browser/window launch. Stopping must preserve the reviewer's
changes; a fresh baseline is a separate, explicit operation.

Smoke-test the adapted setup before presenting it as ready. Confirm entry points, initial
state, readiness and shutdown; verify restart retains modified test files. An unmodified
generated script is only a scaffold. Record the actual source/artifact and environment,
including uncommitted changes, rather than treating creation-time metadata as proof.

## Hand off and follow up

Give the entry point, short paired actions/expected results, stop/resume instructions and
known coverage boundaries. The reviewer should not need to edit application configuration.
Ask for the step number, actual outcome and a screenshot when helpful, not full logs with
tokens. Separate setup checks, human results and not-run items in the final report.

Follow up on reported issues within the implementation's scope. Promote a reusable helper,
fixture or regression test into tracked source when real use justifies it; do not permanently
add every generated packet. Never mark acceptance complete before the human reports results.
