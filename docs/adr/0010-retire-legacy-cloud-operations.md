# Require migration before using a legacy cloud library

Legacy cloud libraries support only the inspection and access needed to migrate into the current format. Ordinary cloud operations, including connection write probes, cannot modify or consume the legacy library. Existing migration and join flows provide the upgrade path; local backup and restore remain available.

Migration preserves remote history without automatically enrolling remote-only Games on the migrating Device. Removing a Game locally does not request deletion of its old cloud history, so no deferred deletion ledger is needed. Shared history can be explicitly deleted after migration. Destructive local Game deletion requires entering `yes`, with confirmation text that distinguishes local backups from live save files and cloud history.
