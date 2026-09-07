# Snapshot extension data

Choose storage according to how the data is accessed:

| Location                                   | Intended data                                                              |
| ------------------------------------------ | -------------------------------------------------------------------------- |
| Archive contents                           | Data that travels with a snapshot and is read during restore or inspection |
| Small `Backups.json` fields                | Frequently read metadata such as archive hashes and creation source        |
| `save_data/<game>/extra_info/<extension>/` | Larger extension data that is read or written independently                |

The core currently exposes `extra_info_dir`, `extra_info_namespace_dir`, and
`extra_info_namespace_file` in `crates/rgsm-core/src/backup/extra_info.rs`.
These helpers validate extension namespaces and file names. Each extension
owns its data schema and any `manifest.json` within its directory.

Future archive extensions should use a reserved directory, such as `__rgsm__/`,
to separate extension data from game save units. This archive convention and
plugin lifecycle integration remain design proposals.

Plugin configuration, caches, and account state have a separate lifecycle from
snapshot attachments and need their own storage design.
