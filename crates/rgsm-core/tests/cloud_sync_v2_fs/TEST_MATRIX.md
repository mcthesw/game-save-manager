# Cloud Sync V2 Fs Test Matrix

This matrix tracks domain behavior exercised through the production OpenDAL Fs
backend. Fine-grained Memory and fake-transport tests remain responsible for
operation-level failures and retry mechanics.

| Domain invariant | Fs scenario | Status |
| --- | --- | --- |
| The selected Fs folder is the exact cloud root | `bootstrap_persists_complete_v2_namespace_across_fresh_operators` | Covered |
| A complete V2 bootstrap survives a fresh Operator | `bootstrap_persists_complete_v2_namespace_across_fresh_operators` | Covered |
| Shared Library and Device Profile normal publication | `single_device_snapshot_round_trip_survives_fresh_operators` | Covered |
| Snapshot metadata, Device Head, and archive bytes persist | `single_device_snapshot_round_trip_survives_fresh_operators` | Covered |
| Local eviction followed by cloud materialization restores exact bytes | `single_device_snapshot_round_trip_survives_fresh_operators` | Covered |
| Reconciliation is idempotent | `single_device_snapshot_round_trip_survives_fresh_operators` | Covered |
| Stale Shared Library replacement cannot overwrite newer state | `two_devices_rebase_stale_library_changes_and_preserve_independent_heads` | Covered |
| Sequential device sync preserves independent Device Heads | `two_devices_rebase_stale_library_changes_and_preserve_independent_heads` | Covered |
| Permanent deletion requires confirmation and leaves a durable Tombstone | `permanent_deletion_tombstones_shared_state_and_converges_other_device_local_bytes` | Covered |
| Another Device removes Tombstoned local bytes after restart | `permanent_deletion_tombstones_shared_state_and_converges_other_device_local_bytes` | Covered |
| Repeating a completed permanent deletion is idempotent | `permanent_deletion_tombstones_shared_state_and_converges_other_device_local_bytes` | Covered |
| Retention deletes only expired automatic Snapshots and keeps the live branch | `retention_removes_only_expired_automatic_snapshot_and_keeps_live_branch` | Covered |
| Device Profile removal preserves other Heads and shared Archive state | `profile_removal_preserves_other_device_head_and_blocks_stale_republication` | Covered |
| A durable Profile marker blocks stale republication | `profile_removal_preserves_other_device_head_and_blocks_stale_republication` | Covered |
| Conflict review and resolution | Follow-up | Deferred |
| Shared Game deletion through its application-service boundary | Follow-up | Deferred |
| Corrupt, missing, partial, and resurrected remote objects | Follow-up | Deferred |
| Interrupted publication/deletion and restart recovery | Follow-up | Deferred |
| Cross-process Cloud Manifest conditional writes | Missing provider CAS contract | Blocked |
| S3 and WebDAV compatibility | Issues #480 and #481 | Out of scope |
