# Require atomic snapshot capture

Status: accepted

Snapshot creation preflights every enabled Save Unit and is rejected before
archive output when resolution is ambiguous, stale, syntactically invalid, or
missing required context. Valid patterns with no current matches may contribute no data,
but an entirely empty operation creates no Snapshot. Any capture I/O failure
rolls back the archive and metadata. Best-effort partial output is not presented
as a trustworthy Snapshot.
