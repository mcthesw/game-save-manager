# Upgrade fixtures

These are synthetic player configurations using historical serialized schemas, not player data.

- `config_v1_7_0.json` follows commit `b549dc0` (the 1.7.0 version bump). This is historical source coverage, not a claim that a 1.7.0 release package was published. Save Units have no IDs; ZIP entries use file/folder names without ID prefixes. `Backups.json` already supports parents and a single `head`
- `config_v1_8_0.json` follows release tag `v1.8.0`. Save Units have stable IDs; V2 ZIP entries use those IDs as prefixes, and snapshot metadata supports per-device heads

The Rust compatibility tests exercise the historical compression methods. Browser upgrade tests seed isolated absolute paths before startup and never repair the migrated configuration or device bindings to make restoration pass.
