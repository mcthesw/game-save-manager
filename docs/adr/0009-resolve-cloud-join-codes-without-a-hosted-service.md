# Resolve cloud join codes without a hosted service

Status: accepted

A Cloud Join Code carries connection information for the player's own cloud
storage and is interpreted by the application without an RGSM-operated code
lookup service. A longer copyable code is acceptable in exchange for avoiding
a separate hosted directory and account dependency. Accessing configuration
through that connection does not make the code itself a configuration backup,
and encoding alone does not protect any credentials it contains.

For 1.9, codes contain the necessary connection credentials without an extra
decryption password. Export and import explicitly warn that anyone holding a
code may obtain the storage permissions of those credentials; codes and secrets
must not appear in logs or error details. Import previews the destination and
requires confirmation before connecting, rather than immediately overwriting
configuration or applying game saves. Both preview and confirmation verify the
existing library identity; a missing or replaced library is never created by import.

Recovery reuses existing shared Games and optionally copies save locations and
launch paths already published by an old Device. It does not add complete
machine-configuration backups or recover local-only Games and preferences that
were never uploaded. Copying locations preserves the current Device identity,
archive folder, local-only Games, and Current Position. Copied Games use Manual
mode with automatic backups disabled until the player reviews their paths.
Resource bindings are remapped to current-Device IDs; ambiguous restore mappings
must be selected again instead of retaining old candidate IDs.
