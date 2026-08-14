//! User-facing compression presets that map to specific ZIP compression methods and levels.

use serde::{Deserialize, Serialize};
use specta::Type;

/// User-facing compression presets for backup archives.
///
/// Each preset maps to a specific `zip::CompressionMethod` and compression level.
/// Old archives using BZip2 remain fully readable regardless of the current preset.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type, utoipa::ToSchema, Default,
)]
pub enum CompressionPreset {
    /// No compression — fastest backup, largest file size.
    Store,
    /// Deflate level 1 — fast with reasonable compression.
    Fast,
    /// Zstd level 3 — best speed/ratio balance (recommended).
    #[default]
    Standard,
    /// Zstd level 19 — very high compression, slower.
    Best,
}

impl CompressionPreset {
    /// Returns the `zip::CompressionMethod` for this preset.
    pub fn zip_method(self) -> zip::CompressionMethod {
        match self {
            Self::Store => zip::CompressionMethod::Stored,
            Self::Fast => zip::CompressionMethod::Deflated,
            Self::Standard | Self::Best => zip::CompressionMethod::Zstd,
        }
    }

    /// Returns the compression level for methods that support it.
    /// `None` means use the library default.
    pub fn compression_level(self) -> Option<i64> {
        match self {
            Self::Store => None,
            Self::Fast => Some(1),
            Self::Standard => Some(3),
            Self::Best => Some(19),
        }
    }

    /// Short identifier used in the archive V2 comment metadata.
    /// Encodes method and level so archives can be distinguished.
    pub fn comment_id(self) -> &'static str {
        match self {
            Self::Store => "stored",
            Self::Fast => "deflate",
            Self::Standard => "zstd:3",
            Self::Best => "zstd:19",
        }
    }
}
