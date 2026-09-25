# Require migration before using a legacy cloud library

Legacy cloud libraries support only the inspection and access needed to migrate into the current format. Ordinary cloud operations, including connection write probes, cannot modify or consume the legacy library. Existing migration and join flows provide the upgrade path; local backup and restore remain available.

Migration preserves remote history without automatically enrolling remote-only Games on the migrating Device. Removing a Game locally does not request deletion of its old cloud history, so no deferred deletion ledger is needed. Shared history can be explicitly deleted after migration. Destructive local Game deletion requires entering `yes`, with confirmation text that distinguishes local backups from live save files and cloud history.

Both the Game management page and Cloud Sync offer deletion by scope: this Device, or the cloud and all Devices. Device-only deletion removes local management and backups, keeps shared history and live save files, and persists the opt-out across metadata refresh. Managing the Game again is an explicit action. Both scopes require entering `yes`; there is no cloud-only Game deletion.
