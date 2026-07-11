# Persist editable restore mappings

Status: accepted

Restore first honors the current Game Device Binding and automatically maps
only when the source-to-target relationship is provably unique. An ambiguous
mapping blocks Apply until the player chooses; that choice is saved by default
as an editable Game-per-Device rule for later restores. Archives record portable
source Capture Groups but do not own target Device configuration. Defaulting to
the first candidate or copying to every candidate was rejected as incorrect.
