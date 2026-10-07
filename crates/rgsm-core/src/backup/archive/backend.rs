use std::path::Path;

#[cfg(test)]
use crate::backup::{CapturePlan, CompressionPreset};

use crate::{
    backup::{RestorePlan, SaveUnit},
    path_resolver::PathContext,
    preclude::*,
};

use super::decompress::RestoreNotifier;

/// Readers for current and historical snapshot archives.
///
/// Each backend handles its own format-specific details: entry layout,
/// metadata storage, compression methods, and permission handling.
pub trait ArchiveBackend {
    /// Compress a fully resolved immutable capture plan as a V3 archive.
    #[cfg(test)]
    fn compress_capture_plan(
        &self,
        plan: &CapturePlan,
        archive_path: &Path,
        preset: CompressionPreset,
        source_fingerprint: Option<String>,
    ) -> Result<u64, CompressError>;

    fn read_capture_manifest(
        &self,
        archive_path: &Path,
    ) -> Result<super::ArchiveManifest, CompressError>;

    fn archive_version(&self, archive_path: &Path) -> Result<super::ArchiveVersion, CompressError>;

    /// Read the source-state fingerprint embedded in a capture archive.
    fn read_source_fingerprint(&self, archive_path: &Path) -> Option<String>;

    fn restore_capture_plan(
        &self,
        plan: &RestorePlan,
        archive_path: &Path,
    ) -> Result<(), CompressError>;

    /// Decompress an archive and restore save units to their original paths.
    fn decompress(
        &self,
        save_units: &[SaveUnit],
        archive_path: &Path,
        notifier: Option<&dyn RestoreNotifier>,
        path_ctx: Option<&PathContext>,
    ) -> Result<(), CompressError>;

    /// File extension for archives created by this backend (without dot).
    #[allow(dead_code)]
    fn extension(&self) -> &str;
}

/// Reader for historical ZIP archives.
pub struct ZipBackend;

/// Reader and current writer for standard 7z archives.
pub struct SevenZBackend;

impl ArchiveBackend for ZipBackend {
    #[cfg(test)]
    fn compress_capture_plan(
        &self,
        plan: &CapturePlan,
        archive_path: &Path,
        preset: CompressionPreset,
        source_fingerprint: Option<String>,
    ) -> Result<u64, CompressError> {
        super::compress::compress_capture_plan_to_file(
            plan,
            archive_path,
            preset,
            source_fingerprint,
        )
    }

    fn read_capture_manifest(
        &self,
        archive_path: &Path,
    ) -> Result<super::ArchiveManifest, CompressError> {
        super::decompress::read_capture_manifest(archive_path)
    }

    fn archive_version(&self, archive_path: &Path) -> Result<super::ArchiveVersion, CompressError> {
        super::decompress::archive_version(archive_path)
    }

    fn read_source_fingerprint(&self, archive_path: &Path) -> Option<String> {
        crate::backup::state_fingerprint::read_stored_fingerprint(archive_path)
    }

    fn restore_capture_plan(
        &self,
        plan: &RestorePlan,
        archive_path: &Path,
    ) -> Result<(), CompressError> {
        super::decompress::restore_capture_plan(plan, archive_path)
    }

    fn decompress(
        &self,
        save_units: &[SaveUnit],
        archive_path: &Path,
        notifier: Option<&dyn RestoreNotifier>,
        path_ctx: Option<&PathContext>,
    ) -> Result<(), CompressError> {
        super::decompress::decompress_from_archive(save_units, archive_path, notifier, path_ctx)
    }

    fn extension(&self) -> &str {
        "zip"
    }
}

impl SevenZBackend {
    #[cfg(test)]
    pub fn compress_capture_plan(
        &self,
        plan: &CapturePlan,
        archive_path: &Path,
        preset: CompressionPreset,
        source_fingerprint: Option<String>,
    ) -> Result<u64, CompressError> {
        super::seven_z::compress_capture_plan(plan, archive_path, preset, source_fingerprint)
    }
}

impl ArchiveBackend for SevenZBackend {
    #[cfg(test)]
    fn compress_capture_plan(
        &self,
        plan: &CapturePlan,
        archive_path: &Path,
        preset: CompressionPreset,
        source_fingerprint: Option<String>,
    ) -> Result<u64, CompressError> {
        self.compress_capture_plan(plan, archive_path, preset, source_fingerprint)
    }

    fn read_capture_manifest(
        &self,
        archive_path: &Path,
    ) -> Result<super::ArchiveManifest, CompressError> {
        super::seven_z::read_manifest(archive_path)
    }

    fn archive_version(&self, archive_path: &Path) -> Result<super::ArchiveVersion, CompressError> {
        Ok(match super::seven_z::read_manifest(archive_path)?.version {
            4 => super::ArchiveVersion::V4,
            5 => super::ArchiveVersion::V5,
            _ => super::ArchiveVersion::V6,
        })
    }

    fn read_source_fingerprint(&self, archive_path: &Path) -> Option<String> {
        super::seven_z::read_manifest(archive_path)
            .ok()
            .and_then(|manifest| manifest.source_fingerprint)
    }

    fn restore_capture_plan(
        &self,
        plan: &RestorePlan,
        archive_path: &Path,
    ) -> Result<(), CompressError> {
        super::seven_z::restore_capture_plan(plan, archive_path)
    }

    fn decompress(
        &self,
        _save_units: &[SaveUnit],
        _archive_path: &Path,
        _notifier: Option<&dyn RestoreNotifier>,
        _path_ctx: Option<&PathContext>,
    ) -> Result<(), CompressError> {
        Err(CompressError::Unexpected(anyhow::anyhow!(
            "Archive V4 restore requires a Restore Plan"
        )))
    }

    fn extension(&self) -> &str {
        "7z"
    }
}
