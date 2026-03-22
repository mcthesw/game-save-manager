use std::path::Path;

use crate::{
    backup::{CompressionPreset, SaveUnit},
    preclude::*,
};

use super::decompress::RestoreNotifier;

/// Abstraction over archive backends (ZIP, future TAR, etc.).
///
/// Each backend handles its own format-specific details: entry layout,
/// metadata storage, compression methods, and permission handling.
pub trait ArchiveBackend {
    /// Compress save units into an archive file.
    /// Returns the compressed file size in bytes.
    fn compress(
        &self,
        save_units: &[SaveUnit],
        archive_path: &Path,
        preset: CompressionPreset,
    ) -> Result<u64, CompressError>;

    /// Decompress an archive and restore save units to their original paths.
    fn decompress(
        &self,
        save_units: &[SaveUnit],
        archive_path: &Path,
        notifier: Option<&dyn RestoreNotifier>,
    ) -> Result<(), CompressError>;

    /// File extension for archives created by this backend (without dot).
    #[allow(dead_code)]
    fn extension(&self) -> &str;
}

/// ZIP-based archive backend (the default and currently only implementation).
pub struct ZipBackend;

impl ArchiveBackend for ZipBackend {
    fn compress(
        &self,
        save_units: &[SaveUnit],
        archive_path: &Path,
        preset: CompressionPreset,
    ) -> Result<u64, CompressError> {
        super::compress::compress_to_file(save_units, archive_path, preset)
    }

    fn decompress(
        &self,
        save_units: &[SaveUnit],
        archive_path: &Path,
        notifier: Option<&dyn RestoreNotifier>,
    ) -> Result<(), CompressError> {
        super::decompress::decompress_from_archive(save_units, archive_path, notifier)
    }

    fn extension(&self) -> &str {
        "zip"
    }
}
