# A/B cloud and text example

Editable starting point, not a required scenario catalog or a completed acceptance
About 5 minutes once the environment is ready · Windows · HTTP-only host + built web GUI
Does not cover desktop windows/tray, packaging, Linux, WebDAV or S3

## Start and initial state

Run `node .rgsm-dev/acceptance/<name>/run.mjs` from this checkout (Node 24, pnpm dependencies and Rust toolchain required)
The first build may take longer; subsequent launches reuse Cargo's incremental output
No windows open automatically · open the printed A/B GUI and text-panel links yourself

Both devices have **Echo Keep**, no snapshots, different text saves, and the same empty local Fs cloud
Use the panel to simulate playing by editing a file and to read the actual result of restoring it
The launcher does not create a cloud library, snapshots, or transfer/restore them for you

## Example journey — trim or replace for the actual change

1. A: open Sync settings and create the cloud library, then open Echo Keep and create a snapshot named `A checkpoint`
   Expected: the snapshot appears locally and in the cloud (use Upload if the transfer has not completed)
2. B: open Sync settings and connect if prompted, then open Echo Keep, download `A checkpoint` and apply it
   Expected: reading B's file in its panel shows `A: starting progress`, while A is unchanged
3. B: write `B: new progress` in its panel, create/upload another snapshot in RGSM, then download/apply it on A
   Expected: A's panel reads `B: new progress`; earlier snapshots remain selectable
4. Stop using either panel's **Stop entire session** button or Ctrl+C, then run the same launcher
   Expected: new links open the same saved progress and snapshot history, not a reset baseline

For a fresh baseline generate a differently named packet; do not delete or reseed the current one
`session.json` describes packet creation; `evidence/runtime.json` records the launch revision/artifact and dirty files
Local logs/configuration can contain runtime credentials — do not paste them into issues

## Result

Not run · record step, observed result and useful screenshots under `evidence/`
Keep automated setup checks, human results and untested environments separate
