# Synchronization model

**Status:** design in progress

This document records the product model while the synchronization experience is being redesigned. Confirmed decisions are normative; open questions must not be implemented as assumptions.

## User journeys

### Occasional archive user

The player occasionally captures save data and usually uploads only selected Snapshots. The application must not turn local capture into automatic Archive upload.

### Multi-device continuity user

The player regularly switches between a PC and a handheld. Selected overlapping Games need automatic cross-device continuity because the platform's native cloud save is unavailable or unreliable.

### High-frequency recovery user

The player keeps automatic local capture enabled for rollback and corruption recovery. This creates substantially more data than occasional capture, so local retention and Archive transfer must remain independent choices.

### Partially overlapping devices

The player owns multiple Devices, but only some Games overlap. Cloud sync must be enabled independently per Game and Device; one Device must not be forced to synchronize the full Shared Library.

Connecting, migrating, or refreshing a cloud library preserves Games that exist only locally, including their save paths and backups. Absence from a cloud directory is not a deletion request. Local-only definitions remain separate from the accepted Shared Library, so ordinary local edits do not publish them or require a working cloud connection.

An existing cloud library connects automatically from saved settings. Cloud Games appear without a separate join step. Equal definitions with the same stable identity need no choice; different identities remain distinct even when their names match. If a local definition and the cloud definition disagree for the same identity, the local version stays usable until the player selects one whole definition. Transfers and shared deletion records exclude that Game until the choice is made; unrelated Games continue normally. Choosing a definition does not apply a Snapshot to live save files.

## Domain model

Synchronization has one per-Game on/off switch plus a remembered cloud preset. Capture, Archive transfer, remote Apply, visibility, local Archive presence, and deletion remain distinct concepts even when presets keep the ordinary UI simple.

### Snapshot record

The shared metadata for one progress point: identity, parent, description, creation source, Archive identity, and availability reports. A Snapshot record may exist without a Cloud Archive.

### Archive copy

The actual zip or 7z bytes for one Snapshot. Copies may independently exist on the current Device, other Devices, and cloud storage.

### Current Position

Each cloud-enabled Device advertises its own pointer into the shared Snapshot graph. There is no global HEAD. A Current Position may reference a Snapshot whose Archive is available only on one Device.

### Per-Game cloud switch

Cloud sync is either disabled or enabled independently for each Game and Device. Disabled cloud sync performs no cloud I/O for the Game while local capture, restore, live save data, Local Archives, and Extra Backups continue unchanged.

Re-enabling immediately publishes accumulated Snapshot records and the Device's Current Position before resuming the remembered cloud preset. It does not itself upload Archive bytes. A failed publication is retried after a later backup or reconnection.

### Capture policy

Controls when local Snapshots are created: manually, on a schedule, or from process-aware automation. Capture policy does not imply Archive transfer.

### Archive transfer policy

Controls whether Archive bytes are transferred manually or automatically. Publishing Snapshot records and Current Position is not Archive upload.

### Remote Apply policy

Controls whether compatible remote progress is only shown, requires confirmation, or may be applied automatically with the required safety backup.

## Archive availability

The Snapshot Catalog can know a Snapshot even when the current Device and cloud storage have no Archive copy.

| Current Device | Cloud | Meaning |
| --- | --- | --- |
| Available | Unavailable | Created or retained locally; not uploaded or intentionally local-only |
| Unavailable | Unavailable | Snapshot is known; another Device may have last reported an Archive copy. If no Device reports one, the Archive is unavailable and requires explicit error treatment. |
| Unavailable | Available | Uploaded by another Device, or evicted locally after upload |
| Available | Available | Current Device and cloud storage both have verified copies |

Availability is a property of each Snapshot, not a single Game-level state. The synchronization overview aggregates counts; the existing Game management page remains the per-Snapshot surface.

## Manual sync behavior

For a cloud-enabled Device, creating a local Snapshot in Manual mode publishes the required Snapshot records and the Device's Current Position. It does not automatically upload Archive bytes.

This allows other Devices to see where progress exists without making that progress restorable from cloud storage. The UI must distinguish `progress known` from `Archive available`.

If cloud sync was disabled or the backend was unreachable, re-enabling or reconnecting publishes accumulated progress without uploading Archives.

## User-facing cloud presets

Ordinary users enable cloud sync and choose one of three remembered presets:

| Preset | Progress records | New local Archives | Remote Archives and live save |
| --- | --- | --- | --- |
| Manual | Publish automatically | Transfer on demand | Explicit download and Apply only |
| Cloud Backup | Same as Manual | Upload automatically | Explicit download and Apply only |
| Multi-device Sync | Same as Cloud Backup | Upload automatically | Download only the unique Forward Target and perform protected Automatic Apply |

Local capture automation remains a separate Game setting. Scheduled capture and process-exit capture do not imply any cloud preset.

The presets are additive user intents, not exposed upload/download direction matrices. No preset mirrors all historical Archives to every Device. `Download all snapshots` remains an explicit **Materialize All** action. Device path and restore mappings are prerequisites for capture and Apply, not synchronization modes. Multi-device Sync follows the unique compatible Forward Target across Device heads rather than a named Device or fixed head.


## Conflict model

Different Snapshot lists, different positions on the same Branch, or an unuploaded Archive do not by themselves constitute a conflict.

When one participating Device's Current Position is an ancestor of another's, the descendant is compatible forward progress. The older Device shows an available update; Multi-device Sync may advance when its safety conditions are satisfied, while other presets require an explicit Apply. If the descendant Archive is unavailable, the Device waits for an Archive copy instead of opening conflict review.

A progress comparison is required only for a true divergence: Device positions on distinct Branches where neither position is an ancestor of the other.

When Multi-device Sync detects true divergence, it enters a suspended runtime state. Automatic Archive upload, receipt, and Apply stop, but the configured preset remains Multi-device Sync. Resolving the divergence automatically resumes normal synchronization.

The comparison flow offers:

- publish the current Device's progress without forcing Archive upload;
- accept a selected remote progress when its Archive is available and protected Apply can succeed;
- decide later without changing state.

A single Device whose local Current Position is ahead of its own stale advertised position is accumulated publication work, not a conflict. It must not receive a red conflict status.

Snapshot identity disagreement is separate from progress divergence. Stale local catalog metadata may self-heal only after the local Archive is recomputed and matches the shared integrity. Different bytes remain a hard identity conflict.

## Game overview

The existing synchronization overview becomes the cross-Game control plane for the Shared Library. It is not another Snapshot browser and has no nested Game detail view.

Each Game row summarizes only:

- cloud-enabled Devices and the selected cloud preset;
- Device progress state;
- four mutually exclusive Archive counts for current-Device local availability × Cloud Archive availability;
- actionable synchronization problems.

The existing Game management page remains the only surface for concrete Snapshot history and Snapshot-level actions such as capture, Apply, on-demand transfer, local eviction, and deletion.

The distribution is rendered as a two-by-two matrix: local available/cloud unavailable, local unavailable/cloud unavailable, local unavailable/cloud available, and both available. The unavailable/unavailable cell distinguishes another Device's last-reported copy from a Snapshot with no known Archive copy. The latter is an actionable data-unavailability error.

## Removal semantics

Disabling cloud sync, evicting a Local Archive, deleting a Snapshot globally, and permanently deleting a shared Game are different operations and must not share labels or implicit behavior.

Local Archive eviction and Cloud Archive removal are always available after an explicit consequence prompt; the application does not force a replacement upload or download. The prompt describes the resulting distribution, warns when only another Device's last-reported copy remains, and gives a stronger warning when no known Archive copy will remain. Eviction never removes the Snapshot record or creates a Tombstone.

The synchronization Game overview contains the per-Game cloud switch and is the only surface for Permanent Shared Game Deletion. Disabling cloud sync is an ordinary reversible setting, not a removal action or deletion-dialog choice.

The Game management page contains no Permanent Shared Game Deletion entry; it is limited to Game configuration and Snapshot-level operations.

## Global Snapshot deletion

Global deletion is distinct from Local Archive eviction and Cloud Archive removal. It first commits a durable Tombstone, then removes available Archive copies locally and in cloud storage. Offline Devices process the Tombstone on their next connection, delete matching Local Archives, and cannot resurrect the Snapshot.

If the initiating Device's Current Position targets the Snapshot, deletion requires an explicit precondition choice: protected Apply to another Snapshot, capture the current live save as a new Snapshot, or clear Current Position while preserving live save data.

Matching positions on other Devices do not block deletion. They are cleared when those Devices observe the Tombstone; Multi-device Sync remains suspended until each affected Device selects an existing Snapshot or captures its current live save. No position automatically falls back to an ancestor.


## External precedents

- [Steam Cloud](https://partner.steamgames.com/doc/features/cloud?l=english) uses global and per-Game enablement without a separate Device-membership lifecycle.
- [Syncthing](https://docs.syncthing.net/v1.29.0/intro/gui.html) treats pause as retained configuration and local data with synchronization activity stopped.
- [Dropbox global deletion](https://help.dropbox.com/delete-restore/delete-files) and [OneDrive Files On-Demand](https://support.microsoft.com/en-us/office/save-disk-space-with-onedrive-files-on-demand-for-windows-0e6860d3-d9f3-4971-b321-7092438fb38e) distinguish account-wide deletion from local-copy eviction.
- [Syncthing deletion propagation](https://docs.syncthing.net/v1.22.2/users/syncing.html) motivates a stricter durable Tombstone so stale offline Devices cannot resurrect a deleted Snapshot.
- [Git branch deletion](https://git-scm.com/docs/git-branch/2.50.0.html) protects a checked-out worktree, supporting an explicit initiating-Device Current Position decision before deletion.
- [Dropbox selective sync](https://help.dropbox.com/sync/selective-sync-overview) removes local copies when deselected, demonstrating why local eviction must remain separate from sync enablement.
- [Ludusavi](https://github.com/mtkennerly/ludusavi/blob/master/docs/cli.md) keeps local backup and restore independent from cloud checks and transfer.
- [Syncthing folder types](https://docs.syncthing.net/v1.27.2/users/foldertypes.html) expose send/receive direction because Syncthing is a generic replication tool; RGSM intentionally keeps those mechanics behind intent presets.
- [Steam Cloud](https://partner.steamgames.com/doc/features/cloud?l=english) uploads changed current saves after play and downloads necessary current saves before play rather than mirroring historical versions.
- [OneDrive Files On-Demand](https://support.microsoft.com/en-us/office/save-disk-space-with-onedrive-files-on-demand-for-windows-0e6860d3-d9f3-4971-b321-7092438fb38e) separates namespace visibility from byte materialization and keeps full offline availability as a separate intent.

## Confirmed decisions

- Keep the ordinary UI simple; implementation axes may be represented through a small number of presets.
- Each Game and Device has one cloud-sync switch plus one remembered preset: Manual, Cloud Backup, or Multi-device Sync.
- Disabling cloud sync performs no cloud I/O but leaves local capture, restore, live save data, and Local Archives unchanged.
- Re-enabling cloud sync immediately publishes accumulated progress before resuming the remembered preset.
- Manual mode publishes progress metadata and Current Position but does not automatically upload Archives.
- The three presets are additive: Manual handles records, Cloud Backup adds automatic upload, and Multi-device Sync adds only the Forward Target download and protected Apply required for convergence.
- No preset automatically downloads all historical Archives; Materialize All is explicit.
- Device mappings are prerequisites, not modes, and Multi-device Sync never binds to one named Device or fixed head.
- Local capture automation remains independent from cloud behavior.
- Unuploaded progress is not a conflict.
- Different positions on one Branch are compatible forward progress, not a conflict.
- Only mutually unreachable Device positions constitute true divergence.
- True divergence suspends Multi-device Sync without changing the selected preset; resolving it resumes synchronization.
- The comparison dialog handles true progress divergence only.
- The synchronization Game overview is the only surface for Permanent Shared Game Deletion.
- Disabling cloud sync is not Stop Managing or a deletion choice.
- The Game management page contains no Game-level lifecycle deletion entry.
- The Game overview uses four mutually exclusive current-Device × cloud Archive availability counts; unavailable/unavailable distinguishes another Device's last-reported copy from no known copy.
- Global Snapshot Deletion requires the initiating Device to resolve a matching Current Position before deletion; the default is falling back to the deleted Snapshot's parent. Other Devices do not block deletion and select or capture progress after their matching positions are cleared.
- Global Snapshot Deletion moves the initiating Device's Current Position to the deleted Snapshot's parent (or clears it if no parent exists). Other Devices' positions are cleared without automatic ancestor fallback.
- Local and Cloud Archive eviction use consequence warnings rather than requiring a verified replacement copy.
- Evicting the last known Archive leaves the Snapshot visible but unavailable; it is not Global Snapshot Deletion.
