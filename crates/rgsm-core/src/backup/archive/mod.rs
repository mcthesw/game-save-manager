//! Archive creation, extraction, and format versioning for game save backups.
//!
//! This module provides a backend-agnostic [`ArchiveBackend`] trait (currently implemented
//! by [`ZipBackend`]) and handles archive versioning ([`ArchiveVersion`]), compression
//! presets ([`CompressionPreset`]), and structured metadata ([`ArchiveMeta`]).
//!
//! ## Archive versions
//!
//! | Version | Comment marker | Timestamps | Entry layout |
//! |---------|---------------|------------|--------------|
//! | Legacy  | *(none)*      | UTC        | Flat         |
//! | V1      | `RGSM_TS_MODE=LOCAL_V1` | Local | Flat |
//! | V2      | `RGSM_ARCHIVE_V2\n{json}` | Local | Index-prefixed (`{i}/path`) |

mod backend;
mod compress;
mod compression_preset;
mod decompress;
mod manifest;
mod timestamp;
mod version;

pub use backend::{ArchiveBackend, ZipBackend};
pub use compression_preset::CompressionPreset;
pub use decompress::{RestoreNotificationLevel, RestoreNotifier};
pub(crate) use manifest::{ArchiveManifestV3, V3_MANIFEST_ENTRY};
pub(crate) use version::ArchiveMeta;
pub use version::ArchiveVersion;

pub(crate) use timestamp::system_time_to_zip_datetime;

#[cfg(test)]
pub(crate) use compress::{add_directory, compress_to_file};
#[cfg(test)]
pub(crate) use decompress::decompress_from_file;
#[cfg(test)]
pub(crate) use timestamp::{local_result_to_timestamp, zip_datetime_to_system_time};
#[cfg(test)]
pub(crate) use version::V1_COMMENT_MARKER;
