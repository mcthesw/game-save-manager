use std::path::Path;

use crate::{
    backup::{CapturePlan, CompressionPreset, Game, RestorePlan, SaveUnit},
    path_resolver::PathContext,
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
        path_ctx: Option<&PathContext>,
    ) -> Result<u64, CompressError>;

    /// Compress a fully resolved immutable capture plan as a V3 archive.
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
    ) -> Result<super::ArchiveManifestV3, CompressError>;

    /// Normalize ID-less historical layouts at the reader boundary.
    fn read_manifest_for_game(
        &self,
        archive_path: &Path,
        _game: &Game,
    ) -> Result<super::ArchiveManifestV3, CompressError> {
        self.read_capture_manifest(archive_path)
    }

    fn archive_version(&self, archive_path: &Path) -> Result<super::ArchiveVersion, CompressError>;

    /// Read the source-state fingerprint embedded in a capture archive.
    fn read_source_fingerprint(&self, archive_path: &Path) -> Option<String>;

    fn restore_capture_plan(
        &self,
        plan: &RestorePlan,
        archive_path: &Path,
    ) -> Result<(), CompressError>;

    /// File extension for archives created by this backend (without dot).
    #[allow(dead_code)]
    fn extension(&self) -> &str;
}

/// ZIP-based archive backend (the default and currently only implementation).
pub struct ZipBackend;

/// Standard 7z Archive V4 backend.
pub struct SevenZBackend;

impl ArchiveBackend for ZipBackend {
    fn compress(
        &self,
        save_units: &[SaveUnit],
        archive_path: &Path,
        preset: CompressionPreset,
        path_ctx: Option<&PathContext>,
    ) -> Result<u64, CompressError> {
        super::compress::compress_to_file(save_units, archive_path, preset, path_ctx)
    }

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
    ) -> Result<super::ArchiveManifestV3, CompressError> {
        super::decompress::read_capture_manifest(archive_path)
    }

    fn read_manifest_for_game(
        &self,
        archive_path: &Path,
        game: &Game,
    ) -> Result<super::ArchiveManifestV3, CompressError> {
        match self.archive_version(archive_path)? {
            super::ArchiveVersion::Legacy | super::ArchiveVersion::V1 => {
                super::legacy_layout::read_flat_zip_manifest(game, archive_path)
            }
            _ => self.read_capture_manifest(archive_path),
        }
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

    fn extension(&self) -> &str {
        "zip"
    }
}

impl ArchiveBackend for SevenZBackend {
    fn compress(
        &self,
        _save_units: &[SaveUnit],
        _archive_path: &Path,
        _preset: CompressionPreset,
        _path_ctx: Option<&PathContext>,
    ) -> Result<u64, CompressError> {
        Err(CompressError::Unexpected(anyhow::anyhow!(
            "Archive V4 requires a preflighted Capture Plan"
        )))
    }

    fn compress_capture_plan(
        &self,
        plan: &CapturePlan,
        archive_path: &Path,
        preset: CompressionPreset,
        source_fingerprint: Option<String>,
    ) -> Result<u64, CompressError> {
        super::seven_z::compress_capture_plan(plan, archive_path, preset, source_fingerprint)
    }

    fn read_capture_manifest(
        &self,
        archive_path: &Path,
    ) -> Result<super::ArchiveManifestV3, CompressError> {
        let manifest = super::seven_z::read_manifest(archive_path)?;
        Ok(super::ArchiveManifestV3 {
            version: manifest.version,
            groups: manifest.groups,
        })
    }

    fn archive_version(&self, archive_path: &Path) -> Result<super::ArchiveVersion, CompressError> {
        Ok(
            if super::seven_z::read_manifest(archive_path)?.version == 5 {
                super::ArchiveVersion::V5
            } else {
                super::ArchiveVersion::V4
            },
        )
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

    fn extension(&self) -> &str {
        "7z"
    }
}
