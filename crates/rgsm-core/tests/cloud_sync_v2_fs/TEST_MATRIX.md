# Cloud Sync V2 Fs Test Matrix

This matrix tracks domain behavior exercised through the production OpenDAL Fs
backend. Fine-grained Memory and fake-transport tests remain responsible for
operation-level failures and retry mechanics.

| Domain invariant | Fs scenario | Status |
| --- | --- | --- |
| The selected Fs folder is the exact cloud root | `bootstrap_persists_complete_v2_namespace_across_fresh_operators` | Covered |
| A complete V2 bootstrap survives a fresh Operator | `bootstrap_persists_complete_v2_namespace_across_fresh_operators` | Covered |
| Shared Library and Device Profile normal publication | Normal single-device path | Planned in this PR |
| Snapshot metadata, Device Head, and archive bytes persist | Normal single-device path | Planned in this PR |
| Reconciliation is idempotent | Normal single-device path | Planned in this PR |
| Stale Shared Library replacement cannot overwrite newer state | Two-device interleaving | Planned in this PR |
| Sequential device sync preserves independent Device Heads | Two-device interleaving | Planned in this PR |
| Conflict review and resolution | Follow-up | Deferred |
| Retention, deletion, tombstones, and profile removal | Follow-up | Deferred |
| Corrupt, missing, partial, and resurrected remote objects | Follow-up | Deferred |
| Interrupted publication/deletion and restart recovery | Follow-up | Deferred |
| Cross-process Cloud Manifest conditional writes | Missing provider CAS contract | Blocked |
| S3 and WebDAV compatibility | Issues #480 and #481 | Out of scope |
