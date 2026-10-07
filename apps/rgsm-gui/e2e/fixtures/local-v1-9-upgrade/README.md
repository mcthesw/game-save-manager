# Local 1.9 upgrade fixture

Synthetic historical data, not a player's configuration or an installed-release test.

- `owners.json` follows the split configuration owners and installation resources at `5f02771`. The test writes its three owners directly, with only isolated installation and backup paths substituted. The cloud namespace is V2 but its backend is disabled: this covers loading existing local state, not upgrading an existing 1.9 remote library.
- `archive-v4.7z` follows `_rgsm/manifest-v4.json` at `5f02771`; `archive-v5.7z` follows the relative-expression addition at `205405c`. They deliberately contain no V6 identity or recovery document.
- Both archives contain `11/0/data/profile.sav` (`primary-beta`) and `12/1/data/profile.sav` (`secondary-beta`). Their manifests preserve separate Save Unit IDs, old installation resource `resource:7`, and original paths under `D:/Beta Game`. V5 also records `relativeExpression: "profile.sav"`.
- These small fixed archives were packed with standard 7-Zip using `-t7z -m0=Deflate -mx=6 -ms=off`; running the test does not require 7-Zip or use the current archive writer to create its historical inputs.

The test starts the real host without a flat configuration, restores both historical archives, creates a new backup, explicitly upgrades the old archives, restarts the host, and restores all three again. It checks IDs, notes, parents, current position, favorite and quick-action references, preserved originals, and the configuration rollback copy.
