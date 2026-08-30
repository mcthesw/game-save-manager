# Require selection for multiple migrated roots

Status: accepted

Legacy game roots migrate into typed Device Resources. When a legacy pattern
uses `<root>` and its Device declared exactly one game root, migration preserves
that uniquely implied choice as a Game Device Binding so later auto-detected
resources cannot make the upgraded path ambiguous. Multiple legacy roots remain
ambiguous and require the player to choose; the old resolver's implicit use of
the first root is never promoted. This may pause an existing automatic backup,
but avoids preserving a path the player never explicitly confirmed.
