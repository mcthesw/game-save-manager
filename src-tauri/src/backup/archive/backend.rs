//! Backend trait and implementations for archive operations.
//!
//! [`ArchiveBackend`] abstracts compress/decompress operations so that future backends
//! (e.g., TAR with Unix permission preservation) can be added without changing callers.

use std::path::Path;

use tauri::AppHandle;

use crate::{
    backup::{CompressionPreset, SaveUnit},
    preclude::*,
};

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
        app_handle: Option<&AppHandle>,
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
        app_handle: Option<&AppHandle>,
    ) -> Result<(), CompressError> {
        super::decompress::decompress_from_archive(save_units, archive_path, app_handle)
    }

    fn extension(&self) -> &str {
        "zip"
    }
}
