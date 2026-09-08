# Acceptance proposal

Suggested outline, not a required format; delete sections that do not help this change

Not ready for review until adapted to the actual change

## Purpose and environment

Describe the changed user journey, why manual acceptance adds value, required platform,
estimated effort, and whether the frontend is development, built web output, or desktop
Record the tested revision and dirty-tree caveat from session.json; refresh this evidence
if the source or binary changes before the reviewer runs it

## Preparation and entry point

Explain the artificial initial state and how to start the prepared run.mjs from the repository
Name any requirements the reviewer must provide; do not claim unavailable platforms were tested
The generated script only seeds an example text file until it has been adapted

## Actions and expected results

Write a few numbered actions in user terms, pairing each with an observable result
Identify device A/B and expected file contents only when the scenario needs them
Keep setup out of the reviewer's steps; overlap with automated tests is fine when useful for human judgment

## Stop and continue

Ctrl+C stops processes owned by the launcher; the session data and evidence remain
Run the same script to continue, not to reset
For a fresh baseline create a new named packet; do not overwrite the reviewer's current session

## Result

Not run

Record environment, step, actual result, and useful screenshots under evidence/
Separate automatic preparation checks from human results and untested boundaries
Do not include runtime tokens, personal saves, or cloud credentials
