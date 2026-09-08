# Restore registry data to device targets

Status: accepted

A registry Save Unit restores its captured subtree to the target configured for
the current Device, rather than treating the archived absolute root as the
restore destination. Use Windows' native `HKEY_CURRENT_USER` root for
current-user locations instead of introducing an RGSM SID variable; explicitly
configured other-user roots must not be silently redirected to the current
user. This keeps old Archives usable after a target change without rewriting
their data or replacing SID strings stored inside registry values.

Saving a location checks only the current Device and reports a missing or
unreadable target without preventing the configuration from being saved.
Malformed locations are distinct from temporarily unavailable ones. RGSM does
not select another Windows account, elevate privileges, or load another user's
registry hive; capture and restore still check their own access requirements.
