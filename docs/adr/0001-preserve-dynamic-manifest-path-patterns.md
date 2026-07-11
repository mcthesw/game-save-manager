# Preserve dynamic manifest path patterns

Status: accepted

Manifest-derived Save Units retain their portable Manifest Path Pattern and
re-evaluate it whenever a backup runs. Each archive records the concrete match
set captured by that backup. Import-time materialization was rejected because
it would miss files created after import, require refreshes when roots or store
users change, and fail to provide the dynamic glob behavior promised by the
upstream format.
