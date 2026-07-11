# Require selection for multiple migrated roots

Status: accepted

Legacy game roots migrate into typed Device Resources, but the old resolver's
implicit use of the first root is not promoted to an explicit Game Device
Binding. One applicable root remains implicitly unique; multiple roots become
ambiguous and require the player to choose. This may pause an existing automatic
backup, but avoids preserving a path the player never explicitly confirmed.
