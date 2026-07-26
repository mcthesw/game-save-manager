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
| Conflict review and resolution | Follow-up | Deferred |
| Retention, deletion, tombstones, and profile removal | Follow-up | Deferred |
| Corrupt, missing, partial, and resurrected remote objects | Follow-up | Deferred |
| Interrupted publication/deletion and restart recovery | Follow-up | Deferred |
| Cross-process Cloud Manifest conditional writes | Missing provider CAS contract | Blocked |
| S3 and WebDAV compatibility | Issues #480 and #481 | Out of scope |
