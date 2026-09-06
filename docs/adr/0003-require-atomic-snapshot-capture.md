# Preflight snapshot capture without a transaction guarantee

Status: accepted

Snapshot creation preflights every enabled Save Unit and is rejected before
archive output when resolution is ambiguous, stale, syntactically invalid, or
missing required context. Valid patterns with no current matches may contribute no data,
but an entirely empty operation creates no Snapshot. Failed or partial output must
not be advertised as a successful Snapshot; retain useful cleanup and report the
error. Cleanup is best effort, not a guarantee of rollback across archive files,
catalogs and cloud metadata. This boundary keeps the application simple and
practical without adding a transaction or recovery framework.
