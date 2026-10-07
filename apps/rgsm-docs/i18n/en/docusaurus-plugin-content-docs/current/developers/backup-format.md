---
title: Reading backup archives
---

New backups use a standard 7z container. Programs read `_rgsm/manifest.json`; people use `RESTORE.txt` for manual recovery. The TXT layout and wording may change and are not a programming interface.

## Format version 6

The manifest is UTF-8 JSON with camelCase fields. Check the top-level `version` first: this document describes version `6`. Do not restore an unknown version using current rules. Ignore unknown additional fields within a supported version.

| Field | Meaning |
| --- | --- |
| `identity.gameId` / `gameName` | Game instance identifier / name at capture time |
| `identity.snapshotId` | Backup identifier; do not derive it from the archive filename |
| `identity.createdAt` | Unix timestamp in milliseconds; may be absent |
| `identity.legacyLocalTime` | Historical local time with an unknown time zone; may be absent |
| `identity.deviceId` / `parent` | Source device / parent backup identifier; may be absent |
| `identity.description` / `createdBy` | Note / creation source, such as `Manual` or `Timer` |
| `identity.recoveredMetadata` | `true` for metadata reconstructed during conversion; defaults to `false` when absent |
| `identity.locations` | Save entry `saveUnitId` and original path `expression` |
| `groups` | Captured data groups; one save entry may match multiple groups |
| `groups[].id` / `saveUnitId` | Group identifier / owning save entry identifier; join by identifiers, not array position |
| `groups[].archivePath` | Relative path inside the archive, using `/` separators |
| `groups[].kind` | `file`, `directory`, or `registry`; registry data uses standard `.reg` files |
| `groups[].relativePath` / `relativeExpression` | Match path relative to its logical save root / expression preserving variables; the latter may be absent |
| `groups[].sourcePathDiagnostic` | Original captured location, for reference only; may be absent |
| `groups[].deleteBeforeApply` | This entry's delete-before-restore setting |

Minimal reader:

```python
import json
from pathlib import Path

# First extract safely into a separate directory using a 7z-capable tool.
root = Path("extracted-backup")
manifest = json.loads((root / "_rgsm/manifest.json").read_text(encoding="utf-8"))
if manifest["version"] != 6:
    raise ValueError("Unsupported backup format")
for group in manifest["groups"]:
    print(group["saveUnitId"], group["kind"], group["archivePath"])
```

`archivePath` points to the actual data; directory groups include their descendants. Colliding names receive numeric suffixes. Use the manifest path instead of reconstructing it from the original filename.

Original paths belong to the source device. Recovery on another device must use that device's corresponding save locations; `sourcePathDiagnostic` is not an executable restore destination. `_rgsm` and `RESTORE.txt` contain backup metadata, not game saves.

Historical ZIP / 7z archives may lack this manifest. Convert them through the application's local backup upgrade first. Missing historical associations require a user choice; matching filenames alone is insufficient.
