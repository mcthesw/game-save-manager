# Game Save Manager

Game Save Manager helps players preserve, restore, and synchronize game progress across devices.

## Language

Each term carries up to three annotations:
- `_Avoid_` — words that are simply wrong or ambiguous; do not use them for this concept.
- `_Player-facing_` — the default word shown to ordinary users in the UI.
- `_Hide by default_` — the underlying technical term; not shown to ordinary users but revealable through a "show technical terms" setting for advanced users and debugging.

**Game**:
A user-managed title whose save data can be backed up and restored.
_Avoid_: entry, project, app

**Game Order**:
The saved display order of **Games** in the main navigation.
_Avoid_: added time, creation time

**Favorites**:
The selected **Games** highlighted by the current **Presentation Mode**.
_Avoid_: Device Visibility, Game Order

**Save List Sort**:
The display strategy used to present **Games** in the save list.
_Avoid_: game order, stored game order

**Save Unit**:
A single file, folder, or registry location that belongs to a **Game**'s save data.
_Avoid_: path, item, source

**Save Unit Type**:
The declared file, folder, or registry kind of a **Save Unit**. A portable pattern retains a known declared type, while an imported pattern without one defers to each **Resolved Save Location**.
_Avoid_: source, resolved kind, archive entry shape

**Save Unit Source**:
The concrete per-Device mapping or portable **Manifest Path Pattern** used to locate a **Save Unit**; it does not replace the unit's declared type.
_Avoid_: Save Unit Type, Resolved Save Location

**Snapshot**:
A point-in-time capture of a **Game**'s enabled **Save Units**, represented as a progress point in the shared history. It may parent later **Snapshots**; when a verified Archive is available, a **Device** can **Apply** it and continue from there.
Its identity is independent of creation time. The original creating **Device**, when known, is preserved across transfers and is distinct from the Devices holding Archive copies.
_Player-facing_: 存档点
_Avoid_: backup, archive, version, memory, 记忆, Device-owned save

**Branch**:
A lineage of **Snapshots** connected by parent pointers, derived implicitly from the snapshot tree; it is not a stored named entity.
_Player-facing_: 进度线
_Hide by default_: branch, 分支

**Current Position**:
A **Device**'s pointer into one **Game**'s shared Snapshot graph; each **Device** keeps its own pointer, so there is no global position or separate per-Device tree.
_Player-facing_: 当前进度
_Hide by default_: HEAD

**Apply**:
Restore a **Game**'s enabled **Save Units** to a **Snapshot** and set the current **Device**'s **Current Position** to it.
_Player-facing_: 恢复到此存档
_Hide by default_: apply, checkout

**Detach**:
Remove a **Snapshot**'s parent pointer so it becomes a new root and its **Branch** stands alone.
_Player-facing_: 从原进度线分离
_Hide by default_: detach

**Extra Backup**:
A local-only, per-**Device**, per-**Game** recovery artifact used to undo an **Apply**, according to the player's Extra Backup preference.
_Player-facing_: 额外备份
_Hide by default_: extra backup, overwrite backup
_Avoid_: snapshot, 存档点

**Device**:
A computer installation with its own game paths, save paths, and progress position.
_Player-facing_: 设备
_Avoid_: machine, host, computer

**Cloud Sync**:
Synchronization of configuration, Snapshots, and opt-in live save convergence through a user-configured storage backend, which may be remote or a local folder.
_Avoid_: upload, remote save, cloud backup as an umbrella term

**Shared Library**:
The portable game definitions and shared preferences understood by every participating **Device**.
_Avoid_: local config, Device Profile

**Local-only Game**:
A **Game** retained by one installation whose definition has not been accepted into its currently connected **Shared Library**. It can have no remote counterpart, or a different local definition awaiting a choice for the same identity. Connecting to or refreshing a cloud library does not remove it or its local backups.
_Avoid_: hidden Game, disabled Game Cloud Sync, deleted Game

**Device Profile**:
One **Device**'s path mappings, resources, presentation, and synchronization policy for its Games, including **Local-only Games**.
_Avoid_: global settings, Shared Library

**Quick Action Profile**:
One **Device Profile**'s complete Quick Action behavior: target **Game** identity, hotkeys, feedback preferences, custom sound resources, and per-Game process automation.
_Avoid_: shared shortcuts, embedded Game definition, split Local State preferences

**Local Archive Root**:
The **Device Profile** location containing one **Device**'s Local Archives and **Extra Backups**.
_Player-facing_: 本地备份位置
_Avoid_: backup_path, Cloud Namespace, another Device's path

**Local State**:
One installation's cloud connection, credentials, interface preferences, **Local-only Game** definitions, and recoverable operational state that never participates in **Cloud Sync**.
_Avoid_: Device Profile, Shared Library, Cloud Manifest

**Cloud Library Bootstrap**:
The connection of one installation to a cloud library. Saved settings connect an existing V2 **Cloud Namespace** automatically; **Cloud Cutover** from V1 and replacement of a different namespace retain their explicit confirmations. Connecting does not choose between conflicting Game definitions or apply Snapshots.
_Avoid_: save cloud settings, ordinary sync, overwrite upload, Bootstrap Choice

**Cloud Join Code**:
Portable connection information, including access credentials, for accessing a player-configured cloud library from another installation. It grants access to existing cloud data rather than containing a complete configuration backup or Snapshot Archives.
_Player-facing_: 云加入码
_Avoid_: official account, Snapshot Archive

**Game Definition Conflict**:
A decision required when one candidate **Game** identity has different proposed and accepted portable definitions.
_Avoid_: Cloud Sync Conflict, Save conflict, automatic merge


**Permanent Shared Game Deletion**:
An explicit irreversible removal of one **Game**, its shared history, and its Archive copies for every **Device**.
_Player-facing_: 从共享云库永久删除
_Avoid_: disable Game Cloud Sync, Device Visibility, Local Archive Eviction

**Game Tombstone**:
The durable shared fact that one stable **Game** identity was permanently deleted and cannot be recreated by stale Device state.
_Avoid_: Archived Game, hidden Game, recycle bin

**Device Path Variable**:
A named path reference declared for the **Shared Library** whose concrete value is supplied independently by each **Device Profile**.
_Avoid_: environment variable, built-in Path Placeholder, shared absolute path

**Device Visibility**:
A **Device Profile** choice that controls whether a **Game** appears in ordinary views on one **Device**.
_Avoid_: sync enabled, cloud availability

**Presentation Mode**:
A **Device Profile** choice between Device-owned Private presentation and the single **Shared Presentation** for **Favorites** and **Game Order**.
_Avoid_: Device reference, Sync Mode, Device Visibility

**Shared Presentation**:
The single **Favorites** and **Game Order** value used by every **Device** whose **Presentation Mode** is Shared.
_Avoid_: source Device, Presentation Profile, shared Device Profile

**Game Cloud Sync**:
A per-**Device**, per-**Game** on/off choice. When disabled, the Device performs no cloud I/O for the Game while local capture, restore, live save data, **Local Archives**, and **Extra Backups** continue unchanged. Re-enabling publishes accumulated Snapshot records and the Device's **Current Position** before applying the remembered **Sync Mode**.
_Player-facing_: 云同步
_Avoid_: Device Participation, Stop Managing, Device Visibility, delete Game, Local Archive Eviction

**Sync Mode**:
A **Device Profile** choice that selects **Manual**, **Cloud Backup**, or **Multi-device Sync** behavior for one cloud-enabled **Game**. The selected mode is remembered while **Game Cloud Sync** is disabled and does not determine when local Snapshots are captured.
_Avoid_: enabled, disabled, visibility, capture schedule, retention policy

**Manual**:
A **Sync Mode** that publishes the current Device's **Snapshot Catalog** progress and **Current Position** after local Snapshot creation, but does not automatically upload Archive bytes or perform unattended **Apply**. Explicit **On-demand Transfer** and **Apply** remain available.
_Avoid_: disabled, offline, hidden, local-only

**Cloud Backup**:
A **Sync Mode** that extends **Manual** by automatically uploading the Archive for each post-activation local **Snapshot**, without downloading remote history or changing live save locations or **Current Position**.
_Player-facing_: 云备份
_Hide by default_: Snapshot Sync
_Avoid_: Cloud Sync, history mirror, Multi-device Sync, Apply

**Multi-device Sync**:
An opt-in **Sync Mode** that extends **Cloud Backup** with prompts about other Devices' progress. The player chooses before downloading and applying a remote Snapshot; neither ancestry nor timestamps authorize unattended changes to live save data.
_Player-facing_: 多端同步
_Avoid_: automatic history download, mirror, force sync, named-Device following

**Process Awareness**:
A **Device**'s ability to report one **Game** as Running, Stopped, or Unknown, used for optional process-aware capture. It is not a prerequisite for cloud synchronization.
_Player-facing_: 游戏运行状态检测
_Avoid_: process required, game installed

**Remote Polling**:
A periodic check for remote metadata while the application is open, independent from **Process Awareness** observation. It supplements checks on startup, connection and return to the application without applying remote progress.
_Player-facing_: 自动检查云端更新
_Avoid_: process polling, game start sync, real-time sync

**Bootstrap Choice**:
An explicit selection that establishes a headless **Device**'s first **Current Position** when local save data exists or multiple distinct remote heads are available.
_Avoid_: latest remote, automatic merge


**Snapshot Catalog**:
The shared description of known **Snapshots**, their parent relationships, and archive identities, independent of archive location.
_Avoid_: local backup list, archive cache

**Cloud Manifest**:
The single versioned cloud file containing every **Game**'s **Snapshot Catalog**, **Tombstone Nodes**, per-Device **Current Position**, last-reported Local Archive presence, and verified **Cloud Archive** availability.
_Avoid_: shared config, task queue, progress log, provider file list

**Cloud Namespace**:
A version-isolated cloud library containing one compatible generation of shared configuration, **Cloud Manifest**, and **Cloud Archives**.
_Avoid_: backend, bucket, Device Profile

**Cloud Library Identity**:
The stable opaque identity of one V2 **Cloud Namespace**, bound locally by each participating installation.
_Avoid_: backend path, content fingerprint, Device identity

**Legacy Cloud Namespace**:
An older **Cloud Namespace** preserved after cutover but excluded from ongoing synchronization by current clients.
_Avoid_: fallback mirror, compatibility replica

**Cloud Cutover**:
The one-time transition that activates a new **Cloud Namespace** and ends mixed-version participation in one shared library.
_Avoid_: continuous migration, dual-write, ordinary sync

**Local Archive**:
The archive bytes for one **Snapshot** that are currently available on a **Device**.
_Avoid_: Snapshot, Snapshot Catalog

**Cloud Archive**:
The archive bytes for one **Snapshot** that are currently available through **Cloud Sync**.
_Avoid_: Snapshot, cloud save

**Local Archive Eviction**:
Removal of one **Local Archive** from one **Device** without deleting the **Snapshot**, its **Cloud Archive**, or another Device's copy.
_Player-facing_: 移除本机副本
_Avoid_: delete Snapshot, cloud deletion, retention deletion

**Cloud Archive Eviction**:
Removal of one **Cloud Archive** without deleting the **Snapshot** or any **Local Archive**.
_Player-facing_: 从云端移除
_Avoid_: delete Snapshot, Global Snapshot Deletion, Local Archive Eviction

**Global Snapshot Deletion**:
A permanent deletion of one **Snapshot** and its available Archive copies, initiated directly or by **Shared Snapshot Retention**, that propagates to offline **Devices** through a **Tombstone Node**. Direct deletion requires the initiating Device to resolve its own matching **Current Position** first; other Devices do not block deletion and lose matching positions when they observe the Tombstone.
_Player-facing_: 从所有设备和云端删除
_Avoid_: local cleanup, missing archive, Local Archive Eviction, automatic ancestor fallback

**Tombstone Node**:
A hidden structural replacement for a globally deleted **Snapshot** that preserves ancestry and prevents stale Devices from resurrecting it, while remaining unavailable for transfer or **Apply**.
_Avoid_: deleted Archive, visible Snapshot, empty Snapshot

**Pending Tombstone**:
A **Tombstone Node** for a permanent deletion whose required Local and Cloud Archive removal has not yet been verified complete.
_Avoid_: Final Tombstone, queued deletion, recoverable Snapshot

**Final Tombstone**:
The permanent minimal **Tombstone Node** remaining after the required Archive deletion has been verified complete.
_Avoid_: garbage-collected Snapshot, deletion history, restorable record

**Shared Snapshot Retention**:
A per-**Game** shared policy that permanently deletes qualifying **Snapshots** and all available Archive copies uniformly across **Devices**.
_Player-facing_: 自动清理共享存档点
_Avoid_: Extra Backup retention, Local Archive Eviction, Device retention

**Retention Protection**:
A shared **Snapshot** choice that excludes one automatic Snapshot from **Shared Snapshot Retention** without changing its creation source.
_Player-facing_: 保留此存档点
_Avoid_: Manual Snapshot, Device favorite, Current Position

**Materialize All**:
An explicit, bounded batch that downloads every **Cloud Archive** currently listed in the **Snapshot Catalog** to one **Device** without Applying any of them.
_Avoid_: full sync, overwrite download

**On-demand Transfer**:
An explicit request to upload or download one available **Snapshot** archive regardless of **Sync Mode**.
_Avoid_: Automatic, startup sync

**Cloud Sync Conflict**:
A synchronization state where cloud-enabled **Devices** advertise **Current Positions** on distinct **Branches**, with neither position ancestral to the other, so progress cannot be advanced safely without a user choice. Different positions on one Branch, missing Archive copies, and unpublished local progress are not conflicts.
_Player-facing_: 设备进度发生分歧
_Hide by default_: merge error, HEAD conflict
_Avoid_: incompatible state, update available, Archive unavailable

**Decide Later**:
A stateless dismissal of the current **Cloud Sync Conflict** interaction that leaves the conflict pending and changes no local or remote data.
_Player-facing_: 稍后处理
_Avoid_: Cancelled resolution, resolved, ignore permanently

**Keep Local**:
A **Cloud Sync Conflict** resolution that keeps the current **Device**'s local **Current Position** and live save data, publishes the required local progress, and preserves all remote-only history and other Device heads.
_Player-facing_: 保留本机进度
_Avoid_: overwrite cloud library, delete remote, global authority

**Accept Remote**:
An explicit, protected **Apply** of one selected remote **Snapshot** that preserves the displaced local lineage and moves **Current Position** only after live save data is updated successfully.
_Player-facing_: 使用所选云端进度
_Avoid_: download, latest cloud, replace backup list

**Ludusavi Manifest**:
An external catalog of games and known save locations used to suggest **Save Units**.
_Avoid_: game database, scanner, importer

**Manifest Path Pattern**:
A portable save-location expression from the **Ludusavi Manifest** that may contain **Path Placeholders** and glob syntax and may expand to zero or more **Resolved Save Locations** on a **Device**.
_Avoid_: path, absolute path, Save Unit

**Path Placeholder**:
A named token such as `<home>` or `<base>` inside a **Manifest Path Pattern** whose value depends on the operating system, store, game, or **Device**.
_Avoid_: environment variable, path variable

**Resolved Save Location**:
A concrete file, folder, or registry location produced by evaluating a **Manifest Path Pattern** for one **Device**.
_Avoid_: manifest path, path variable

**Resolution Selection**:
A **Device**-scoped choice that narrows a **Manifest Path Pattern** to one or more candidate store accounts, installation roots, or game installations.
_Avoid_: global path override, resolved path

**Device Resource**:
A store account, installation root, or game installation that is available on one **Device** for resolving **Manifest Path Patterns**.
_Avoid_: global root, shared path

**Game Device Binding**:
A **Game**'s explicit selection of **Device Resources** on one **Device**, inherited by that **Game**'s manifest-derived **Save Units**.
_Avoid_: Device configuration, Save Unit path override

**Capture Group**:
The concrete data captured from one **Manifest Path Pattern** under one candidate combination in a **Snapshot**.
_Avoid_: absolute source path, Save Unit

**Restore Mapping Rule**:
An editable rule in a **Game Device Binding** that maps a source **Capture Group** identity to target **Device Resources** for later **Apply** operations.
_Avoid_: archive path, one-time restore path

## Relationships

- A **Game** has one or more **Save Units**.
- A **Save Unit Type** describes what data a **Save Unit** contains, while its **Save Unit Source** describes how a Device locates that data.
- Migrating a known concrete **Save Unit** to a portable **Manifest Path Pattern** preserves its declared **Save Unit Type**.
- **Game Order** arranges **Games** for display without changing their save data.
- **Save List Sort** can present **Games** by **Game Order**, last played time, or name.
- A **Device** in Private **Presentation Mode** owns its **Favorites** and **Game Order**; a Device in Shared mode uses the one **Shared Presentation**.
- **Device Visibility** may hide a shared favorite locally but never removes it from **Shared Presentation**.
- A **Save Unit** may define a distinct location for each **Device**.
- A **Game** may define a distinct launch location for each **Device**.
- A **Game** produces zero or more **Snapshots**.
- A **Snapshot** is created only after every enabled **Save Unit** has a valid,
  non-ambiguous capture plan and all matched data is captured successfully.
- A valid **Manifest Path Pattern** matching no data does not by itself make a
  **Snapshot** incomplete, but no **Snapshot** is created when the entire
  **Game** has no matched data.
- A **Snapshot** has at most one parent **Snapshot**; the parent chain forms a **Branch**.
- A **Snapshot** belongs to the shared graph and is not owned by a **Device**.
- A **Snapshot** may describe how it was created without identifying a creator **Device**.
- A **Device** has its own **Current Position** for a **Game**, independent of other **Devices**.
- Within the shared Snapshot graph, a **Device** relates to progress only through its per-Game **Current Position**.
- Every Device's **Current Position** points into the same shared Snapshot graph; a Device does not own a separate tree.
- **Apply** restores a **Snapshot** and moves the current **Device**'s **Current Position** to it.
- **Apply** requires an explicit player choice and may create an **Extra Backup** according to user preference.
- **Detach** turns a **Snapshot** into a new root, splitting its **Branch** from the parent lineage.
- **Cloud Sync** transfers **Snapshots** and shared configuration between **Devices**.
- Every **Device** joined to one **Cloud Namespace** understands the same **Shared Library**.
- Each **Device** has exactly one **Device Profile**.
- The **Shared Library** contains no Device-specific absolute paths or cloud credentials.
- A **Device Profile** supplies one Device's concrete values and desired behavior without changing another Device Profile.
- Each **Device Profile** supplies exactly one **Local Archive Root** for that Device.
- An unavailable **Local Archive Root** never means its Local Archives were deleted.
- A **Device** is the sole normal writer of its own **Device Profile**; another Device may inspect it or copy values into its own Profile.
- Removing a **Device Profile** never removes a **Snapshot**, an Archive, or a node from the shared Snapshot graph.
- Each **Device Profile** owns one cohesive **Quick Action Profile**.
- A **Quick Action Profile** references its target by stable **Game** identity and never embeds the portable Game definition.
- Quick Action hotkeys, feedback choices, custom sound resources, and process automations never transfer implicitly from another **Device Profile**.
- **Local State** belongs to one installation and never becomes part of the **Shared Library**, a **Device Profile**, or the **Cloud Manifest**.
- Saving **Local State** neither starts **Cloud Library Bootstrap** nor mutates shared state.
- **Cloud Library Bootstrap** classifies the remote namespace before mutation and joins an existing V2 **Shared Library** before publishing the current **Device Profile**.
- **Cloud Library Bootstrap** creates a new **Shared Library** only after the player explicitly confirms a genuinely empty **Cloud Namespace**.
- Creating or cutting over a V2 **Cloud Namespace** assigns one **Cloud Library Identity**; joining adopts the existing identity.
- Ordinary V2 cloud operations require the remote **Cloud Library Identity** to match **Local State**; only explicit reconnection may bind a different identity.
- A **Game Definition Conflict** is distinct from a **Cloud Sync Conflict**: the former chooses a whole portable **Game** definition, while the latter chooses save progress.
- A **Game Definition Conflict** may keep the existing shared definition or replace it with the complete local portable definition; it never merges individual fields.
- Deferring a **Game Definition Conflict** changes neither the **Shared Library** nor the unresolved local candidate.
- Concurrent changes to different **Games** may coexist in the **Shared Library**, while concurrent changes to one Game produce a **Game Definition Conflict**.
- Disabling **Game Cloud Sync** changes only the current **Device Profile** and leaves local capture, restore, live save data, the **Game**, every **Snapshot**, and all Archive copies intact.
- Disabled **Game Cloud Sync**, **Device Visibility**, and **Local Archive Eviction** are independent current-Device choices.
- Re-enabling **Game Cloud Sync** publishes accumulated Snapshot records and the Device's **Current Position** before the remembered **Sync Mode** resumes.
- Only **Permanent Shared Game Deletion** removes a **Game** from the **Shared Library**.
- **Permanent Shared Game Deletion** removes the Game's complete Snapshot graph, every **Current Position**, and every available Archive copy.
- A **Game Tombstone** outlives deleted Game state and prevents an offline **Device** from restoring it.
- **Device Visibility** changes presentation without changing **Game Cloud Sync**, **Sync Mode**, or archive availability.
- A **Device Profile** remembers exactly one **Sync Mode** for each **Game**, including while **Game Cloud Sync** is disabled.
- **Manual**, **Cloud Backup**, and **Multi-device Sync** are mutually exclusive per-**Game** states when **Game Cloud Sync** is enabled.
- **Cloud Backup** may change archive availability but never changes live save locations.
- **Cloud Backup** and **Multi-device Sync** upload Archives for local Snapshots added after their activation boundary, including authorized work accumulated while the **Device** was offline; neither mode automatically materializes remote history.
- **Multi-device Sync** discovers updates and divergent progress; the player chooses before on-demand download and **Apply**.
- **Process Awareness** supports optional capture; missing process information does not block synchronization.
- A game-process start updates **Process Awareness** but never restores live save data.
- A game-process exit may create a changed-data **Snapshot** only when that per-**Game** option is enabled; it never restores live save data.
- **Remote Polling**, application-start reconciliation, and explicit synchronization discover remote progress independently from game-process start and exit events.
- **Remote Polling** runs while the application is open, using the configured interval or the default interval when none is configured.
- All remote Device positions remain visible; no Device automatically adopts another's position or live save data.
- Divergence does not stop Archive upload: preserving both branches is separate from choosing which progress to Apply.
- An **Extra Backup** never enters the **Snapshot Catalog**, becomes a **Cloud Archive**, or participates in **Cloud Sync**.
- **Materialize All** never changes live save locations and never moves a **Device**'s **Current Position**.
- Enabling **Cloud Backup** or **Multi-device Sync** never implies **Materialize All**; downloading all Cloud Archives is a separate explicit operation.
- The **Snapshot Catalog** may describe a **Snapshot** with no **Local Archive** on the current **Device**.
- A **Snapshot** may have a **Local Archive**, a **Cloud Archive**, both, or neither while recovery is pending.
- Every V2 **Snapshot** with available Archive bytes has one mandatory content hash shared by its **Local Archive** and **Cloud Archive** copies.
- A **Local Archive** or **Cloud Archive** is verified available only when its size and content hash match the **Snapshot**.
- An **Apply** never uses a V2 Archive whose content hash is missing or mismatched.
- A **Device Path Variable** may have a different concrete value on every **Device**; a missing value never resolves to another Device's path.
- A newly created **Snapshot** may enter the **Cloud Manifest** before its Archive uploads; other Devices may see it but cannot download or Apply it until **Cloud Archive** availability is verified.
- The **Cloud Manifest** may record another Device's last-reported **Local Archive** presence for display, but that record never authorizes or enables a cross-Device transfer.
- Every **Cloud Manifest** change reads the latest state, applies pure merge rules, writes, reads back, verifies its intended facts, and retries when verification fails; strict atomic cross-Device replacement remains a documented TODO.
- A **Cloud Cutover** moves a shared library to one current **Cloud Namespace**; participating **Devices** must use clients compatible with that namespace.
- A **Cloud Cutover** activates only after every accepted legacy **Snapshot** either has a verified **Cloud Archive** in the current **Cloud Namespace** or is explicitly known to have no available Archive.
- Current clients never merge later changes from a **Legacy Cloud Namespace** into the active **Cloud Namespace**.
- An active **Cloud Namespace** never retrieves or repairs a **Cloud Archive** from a **Legacy Cloud Namespace**.
- A **Legacy Cloud Namespace** remains a separate historical view and never acts as a compatibility replica of current progress.
- **On-demand Transfer** changes archive availability without changing **Device Visibility** or **Sync Mode**.
- **Local Archive Eviction** and **Cloud Archive Eviction** change only Archive availability at their respective location.
- Archive eviction remains available even when no replacement copy is verified. Before eviction, the application states the resulting known distribution, warns when only another Device's last report remains, and gives a stronger warning when no known Archive copy will remain.
- Evicting the last known Archive copy leaves the **Snapshot** and its graph position intact but unavailable for transfer or **Apply**; it never implies a **Tombstone Node**.
- **Global Snapshot Deletion** is mandatory shared state; it is never inferred from a missing **Local Archive**, retention cleanup, or **Device Visibility**.
- **Shared Snapshot Retention** initiates **Global Snapshot Deletion** and never performs Device-local-only Snapshot cleanup.
- Every **Device** observes the same **Shared Snapshot Retention** policy for one **Game**.
- **Shared Snapshot Retention** applies its count independently to each live **Branch**.
- One **Branch** never consumes another Branch's retention allowance.
- Every active **Current Position** and every **Retention Protection** excludes its **Snapshot** from **Shared Snapshot Retention**.
- **Retention Protection** never changes how a **Snapshot** was created.
- **Extra Backup** retention belongs to one **Device** and one **Game** and never participates in **Shared Snapshot Retention**.
- **Global Snapshot Deletion** replaces the visible **Snapshot** with a **Tombstone Node**; children retain structural ancestry, while affected **Current Position** references are cleared.
- A **Pending Tombstone** becomes a **Final Tombstone** only after the required Archive removal is verified.
- **Pending Tombstone** and **Final Tombstone** have the same anti-resurrection and ancestry meaning.
- A **Final Tombstone** remains permanently even though completed-operation detail does not.
- A **Tombstone Node** may be traversed for ancestry but can never be materialized, downloaded, or Applied.
- **Global Snapshot Deletion** has no undo and removes available Local and Cloud Archives only after its **Tombstone Node** becomes durable.
- A **Device** that learns a **Tombstone Node** removes its matching **Local Archive** before it can publish or transfer that deleted Snapshot.
- Direct **Global Snapshot Deletion** requires the initiating **Device** whose **Current Position** targets the Snapshot to choose a protected **Apply** to another Snapshot, capture current live save data as a new Snapshot, or explicitly clear its position before deletion.
- Another Device's matching **Current Position** never blocks deletion; it is cleared when that Device observes the **Tombstone Node**, and the Device may select or capture progress without blocking uploads.
- **Global Snapshot Deletion** never moves a **Current Position** automatically to an ancestor and never changes live save data except through an explicitly chosen protected **Apply**.
- Physical Archive removal never removes the corresponding **Tombstone Node**.
- One **Device** never distributes an upload task to another Device; uploads arise only from the acting Device's **Sync Mode**, its own explicit upload action, or retry of its previously authorized work.
- The explicit "download all snapshots to this device" action invokes **Materialize All**; no **Sync Mode** implies it.
- A **Cloud Sync Conflict** is resolved by choosing which progress should become authoritative.
- A **Cloud Sync Conflict** does not create a named branch or require a `Fork` action; parallel progress already remains represented by existing **Snapshots** and per-**Device** **Current Position** values.
- **Decide Later**, closing the conflict dialog, pressing Escape, and closing the containing window all leave the **Cloud Sync Conflict** pending without invoking a resolution command.
- **Keep Local** changes only which head the current **Device** publishes as its authority; it preserves every other Device head and the union of local and remote **Snapshots**.
- **Keep Local** publishes required **Cloud Archives** before committing the merged **Snapshot Catalog**, and it never changes live save data.
- **Accept Remote** materializes and validates one selected remote **Snapshot**, creates a mandatory **Extra Backup** when local save data exists, Applies the Snapshot, and only then moves **Current Position**.
- **Accept Remote** preserves the displaced local Snapshot lineage; it does not delete the progress the player chose not to use.
- **On-demand Transfer** downloads an archive without Applying it, while **Accept Remote** explicitly changes live save data.
- The **Ludusavi Manifest** can suggest **Save Units** for a **Game**.
- A **Manifest Path Pattern** contains zero or more **Path Placeholders**.
- A **Manifest Path Pattern** can produce zero or more **Resolved Save Locations** for one **Device**.
- A **Save Unit** created from a **Manifest Path Pattern** retains that pattern and may resolve to a different set of **Resolved Save Locations** when a backup runs on another **Device** or at another time.
- A **Resolved Save Location** can contribute to a **Save Unit**, but the two are not synonyms.
- A **Resolution Selection** belongs to one **Device** and never rewrites the shared **Manifest Path Pattern**.
- Without a **Resolution Selection**, exactly one candidate is used implicitly; zero candidates remain unresolved, while multiple candidates require an explicit **Resolution Selection**.
- A **Device** exposes zero or more **Device Resources**.
- A **Game Device Binding** selects **Device Resources** for exactly one **Game** on exactly one **Device**.
- A manifest-derived **Save Unit** inherits its **Game Device Binding** and does not duplicate the same selection independently.
- A **Snapshot** records one or more **Capture Groups** without storing target-Device configuration.
- A **Restore Mapping Rule** belongs to a **Game Device Binding**, is reused by later **Apply** operations, and can be changed or removed by the player.

## Example dialogue

> **Dev:** "When I **Apply** a **Snapshot**, do I restore every path listed on the **Game**?"
> **Domain expert:** "No — restore the enabled **Save Units** for the current **Device**; other **Device** locations describe the same **Game** elsewhere."

> **Dev:** "When the user starts a new **Branch** from a **Snapshot**, what do we store?"
> **Domain expert:** "Nothing named. A **Branch** is just the parent chain; 'start a new **Branch**' means create a **Snapshot** whose parent is that node. There is no branch name to manage."

> **Dev:** "Is there one global **Current Position** per **Game**?"
> **Domain expert:** "No — each **Device** has its own **Current Position**. Two **Devices** can sit on different **Snapshots** of the same **Game**."

> **Dev:** "Does downloading another Device's newer Snapshot move my **Current Position**?"
> **Domain expert:** "No — materialization only makes the archive available; Snapshot creation or an **Apply** moves the current Device's pointer."

> **Dev:** "If a restored **Device Profile** points to an unavailable **Local Archive Root**, should we use the default folder?"
> **Domain expert:** "No — require the player to select a valid local backup location; an unavailable root does not prove that any Archive was deleted."

> **Dev:** "Should Quick Action notifications stay in **Local State** while hotkeys and process automation live in the **Device Profile**?"
> **Domain expert:** "No — keep the complete **Quick Action Profile** together because one settings flow and one runtime lifecycle own all of those choices."

> **Dev:** "Which downloaded Snapshot does **Multi-device Sync** Apply when several Devices have heads?"
> **Domain expert:** "The one the player explicitly selects. Neither ancestry nor timestamps prove that live save data can be overwritten."

> **Dev:** "Can a new Device with no **Current Position** choose the farthest of several remote heads?"
> **Domain expert:** "No — without a lineage anchor, multiple distinct heads require a **Bootstrap Choice**, regardless of their ancestry."

> **Dev:** "Must the game be installed or have a known process before enabling **Multi-device Sync**?"
> **Domain expert:** "No — process detection is optional and only required for process-exit capture. Remote Apply is always a player choice."

> **Dev:** "Does detecting that a game started immediately restore remote progress?"
> **Domain expert:** "No — process start only changes **Process Awareness**. Remote progress is discovered by **Remote Polling**, application-start reconciliation, or an explicit sync, and Apply still passes every safety gate."

> **Dev:** "If this **Device** uses **Manual**, does the player lose access to old cloud snapshots?"
> **Domain expert:** "No - **Manual** stops background archive transfer, while **On-demand Transfer** remains available."

> **Dev:** "Does enabling **Cloud Backup** or **Multi-device Sync** download every existing **Cloud Archive**?"
> **Domain expert:** "No — full-history download is a separate explicit **Materialize All** action. Neither mode mirrors remote history."

> **Dev:** "What does **Multi-device Sync** download automatically?"
> **Domain expert:** "Nothing without a player request. Both cloud presets upload new Archives; Multi-device Sync additionally prompts about other Devices' progress."

> **Dev:** "Does resolving a **Cloud Sync Conflict** need a `Fork` button?"
> **Domain expert:** "No — parallel progress is already preserved by **Snapshots** and per-Device **Current Position** values in the shared graph. The MVP has no named branch entity or `Fork` action."

> **Dev:** "Is closing the conflict dialog a `Cancelled` resolution?"
> **Domain expert:** "No — it is **Decide Later**: no domain transition occurs, the conflict stays pending, and no local or remote data changes. Transfer-job cancellation is a separate concept."

> **Dev:** "Does **Keep Local** overwrite the entire cloud history?"
> **Domain expert:** "No — it chooses this Device's local head, preserves remote-only Snapshots and every other Device head, uploads required Archives first, and commits the merged Catalog last."

> **Dev:** "Does **Accept Remote** only download the cloud archive?"
> **Domain expert:** "No — it is an explicit guarded Apply. The UI explains the impact, the service protects existing local data, and Current Position moves only after live save data is updated successfully. Use **On-demand Transfer** to download without Applying."

> **Dev:** "If Device A has a Snapshot locally but has not uploaded its Archive, can Device B download it?"
> **Domain expert:** "No — the **Cloud Manifest** lets B see that the Snapshot exists and that its Cloud Archive is absent, but B never sends an upload task to A. A's own mode, explicit upload, or retry must make it cloud-available first."

> **Dev:** "Can one Device's automatic retention delete only its own copy of a shared **Snapshot**?"
> **Domain expert:** "No — **Shared Snapshot Retention** produces one global deletion result: the Cloud Archive is removed and every Device removes its Local Archive through the same **Tombstone Node**."

> **Dev:** "Does a completed deletion remove its **Tombstone Node**?"
> **Domain expert:** "No — the **Pending Tombstone** becomes a smaller permanent **Final Tombstone** so stale Devices still cannot restore the Snapshot."

> **Dev:** "Does keeping an automatic **Snapshot** turn it into a manual Snapshot?"
> **Domain expert:** "No — **Retention Protection** preserves the Snapshot without rewriting how it was created."

> **Dev:** "If two **Branches** are active, do they compete for one shared retention count?"
> **Domain expert:** "No — each live Branch keeps its own allowance, and incomparable Branches are never ranked by Device time."

> **Dev:** "When a wildcard in a **Manifest Path Pattern** matches a new save file tomorrow, must the player import the game again?"
> **Domain expert:** "No — the **Save Unit** keeps the pattern and resolves it again for each backup."

## Flagged ambiguities

- "backup" is often used casually; use **Snapshot** for a point-in-time saved state and **Cloud Backup** only for the **Sync Mode** that protects Snapshots through cloud storage.
- "application safety backup" means a local-only **Extra Backup**, not a **Snapshot** and not cloud history.
- "branch" sounds like a stored, named object (as in Git); here a **Branch** is implicit, derived from **Snapshot** parent pointers. There is no branch entity or branch name.
- "fork" must not appear as an MVP conflict action; preserving parallel Snapshot lineages does not create a new domain object.
- "cancelled" is ambiguous between dismissing a conflict dialog and stopping an in-flight job; use **Decide Later** for the former and job cancellation for the latter.
- "use cloud progress" must name the selected progress and disclose that live save data will change; it must not be presented as a simple download.
- "available" is incomplete without a location and verification source; distinguish Catalog-known, locally verified, and verified Cloud Archive availability.
- "HEAD" is per-**Device** (**Current Position**), not a single global pointer. Sync conflict resolution talks about local and remote **Current Position** values, not one shared HEAD.
- "my tree" is ambiguous; all Devices share one Snapshot graph and differ only in their **Current Position** pointers and reachable lineages.
- "latest" describes chronological presentation, not which Device's progress should replace live save data.
- "path variable" is commonly used for Ludusavi tokens, but it is easily confused with an OS environment variable; use **Path Placeholder** in domain documentation.
- "resolved path" is often spoken of as singular, but one **Manifest Path Pattern** may produce multiple **Resolved Save Locations**.
- "partial backup" must not be presented as a **Snapshot**; a **Snapshot** is an atomic, trustworthy capture rather than a best-effort collection.
- "sync all snapshots to local" means **Materialize All**, not either automatic **Sync Mode**.
- "automatic sync" is ambiguous; use **Cloud Backup** for automatic upload and **Multi-device Sync** for automatic upload plus player-facing update prompts.
- "startup sync" is ambiguous; say startup reconciliation and name the concrete work it resumes or discovers.
- "polling" is ambiguous; use **Remote Polling** for user-scheduled cloud checks and Process Awareness observation for internal game-process detection.
- "manual sync" is ambiguous; use **Manual** for the persistent **Sync Mode** and **On-demand Transfer** for an explicit action.
- "synced" must not imply local archive presence; consult the **Snapshot Catalog**, **Local Archive**, and **Cloud Archive** separately.
- "hidden" describes **Device Visibility**, not **Sync Mode**.
- "cloud backup" means the **Cloud Backup** mode; do not use it for the **Cloud Sync** umbrella, a single **Cloud Archive**, or an **Apply** operation.
- "keep snapshot" is ambiguous between progress authority and retention; use **Retention Protection** when the intent is only to prevent automatic deletion.
- "completed deletion" removes operation detail, not the anti-resurrection marker; the **Final Tombstone** remains permanent.
- "quick action settings" is one **Quick Action Profile**, not a portable **Game** definition plus unrelated Local State toggles.
