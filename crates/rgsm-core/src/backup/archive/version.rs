//! Archive version detection and structured metadata (V2+).
//!
//! [`ArchiveVersion`] is determined from the ZIP comment and controls archive behavior
//! (timestamp interpretation, entry layout). [`ArchiveMeta`] stores extensible metadata
//! as JSON in the comment body for V2+ archives.

use serde::{Deserialize, Serialize};

use super::compression_preset::CompressionPreset;

/// Header line in ZIP comment identifying V2 RGSM archives.
///
/// The version is encoded in the header itself (e.g., `RGSM_ARCHIVE_V2`),
/// so future versions use a distinct header (`RGSM_ARCHIVE_V3`, etc.).
pub const ARCHIVE_COMMENT_HEADER: &str = "RGSM_ARCHIVE_V2";
pub const ARCHIVE_COMMENT_HEADER_V3: &str = "RGSM_ARCHIVE_V3";

/// Comment marker for V1 archives (local timestamps, flat layout).
pub(crate) const V1_COMMENT_MARKER: &str = "RGSM_TS_MODE=LOCAL_V1";

/// Archive format version, determined from the ZIP comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveVersion {
    /// No RGSM marker — pre-1.0 archives with UTC timestamps, flat layout.
    Legacy,
    /// `RGSM_TS_MODE=LOCAL_V1` — local timestamps, flat layout.
    V1,
    /// `RGSM_ARCHIVE_V2` header + JSON body — local timestamps, index-prefixed layout.
    V2,
    /// `RGSM_ARCHIVE_V3` header + JSON body and an internal capture manifest.
    V3,
    /// Standard 7z container with an internal Archive V4 capture manifest.
    V4,
}

impl ArchiveVersion {
    /// Detect archive version from the ZIP comment bytes.
    pub fn from_comment(comment: &[u8]) -> Self {
        let s = match std::str::from_utf8(comment) {
            Ok(s) => s,
            Err(_) => return Self::Legacy,
        };
        if s.starts_with(ARCHIVE_COMMENT_HEADER_V3) {
            return Self::V3;
        }
        if s.starts_with(ARCHIVE_COMMENT_HEADER) {
            return Self::V2;
        }
        if s == V1_COMMENT_MARKER {
            return Self::V1;
        }
        Self::Legacy
    }

    /// V2+ archives prefix entries with `{save_unit_id}/`.
    pub fn uses_save_unit_prefix(self) -> bool {
        matches!(self, Self::V2 | Self::V3 | Self::V4)
    }

    /// V1+ archives store timestamps in local time; Legacy uses UTC.
    pub fn uses_local_timestamps(self) -> bool {
        matches!(self, Self::V1 | Self::V2 | Self::V3 | Self::V4)
    }

    /// Normalize an archive entry path by stripping the save-unit prefix if present.
    ///
    /// V2+ archives store entries as `{save_unit_id}/{path}` (e.g., `0/save.dat`).
    /// Returns `None` for pure save-unit directory entries (e.g., `0/`) that should be skipped.
    pub fn normalize_entry_path(self, entry_name: &str) -> Option<&str> {
        if !self.uses_save_unit_prefix() {
            return Some(entry_name);
        }
        match entry_name.find('/') {
            Some(pos) => {
                let stripped = &entry_name[pos + 1..];
                if stripped.is_empty() {
                    None
                } else {
                    Some(stripped)
                }
            }
            None => None,
        }
    }
}

/// Structured metadata stored in archive comments (V2+).
///
/// Serialized as JSON in the ZIP comment after the [`ARCHIVE_COMMENT_HEADER`] line.
/// New optional fields can be added without breaking older readers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveMeta {
    pub version: u32,
    pub compression: String,
    /// Source fingerprint at the time of compression, used for timer dedup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_fingerprint: Option<String>,
}

impl ArchiveMeta {
    pub fn new(preset: CompressionPreset) -> Self {
        Self {
            version: 2,
            compression: preset.comment_id().to_string(),
            source_fingerprint: None,
        }
    }

    pub fn new_v3(preset: CompressionPreset) -> Self {
        Self {
            version: 3,
            compression: preset.comment_id().to_string(),
            source_fingerprint: None,
        }
    }

    /// Serialize to a ZIP comment string: header line + JSON body.
    pub fn to_comment(&self) -> String {
        let json = serde_json::to_string(self).expect("ArchiveMeta serialization cannot fail");
        let header = if self.version >= 3 {
            ARCHIVE_COMMENT_HEADER_V3
        } else {
            ARCHIVE_COMMENT_HEADER
        };
        format!("{header}\n{json}")
    }

    /// Parse from a ZIP comment. Returns `None` for non-V2 archives.
    #[allow(dead_code)]
    pub fn from_comment(comment: &[u8]) -> Option<Self> {
        let s = std::str::from_utf8(comment).ok()?;
        let json_body = s
            .strip_prefix(ARCHIVE_COMMENT_HEADER_V3)
            .or_else(|| s.strip_prefix(ARCHIVE_COMMENT_HEADER))?
            .trim_start_matches('\n');
        serde_json::from_str(json_body).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_from_empty_comment() {
        assert_eq!(ArchiveVersion::from_comment(b""), ArchiveVersion::Legacy);
    }

    #[test]
    fn version_from_v1_comment() {
        let comment = V1_COMMENT_MARKER.as_bytes();
        assert_eq!(ArchiveVersion::from_comment(comment), ArchiveVersion::V1);
    }

    #[test]
    fn version_from_v2_comment() {
        let meta = ArchiveMeta::new(CompressionPreset::Standard);
        let comment = meta.to_comment();
        assert_eq!(
            ArchiveVersion::from_comment(comment.as_bytes()),
            ArchiveVersion::V2
        );
    }

    #[test]
    fn version_from_v3_comment() {
        let meta = ArchiveMeta::new_v3(CompressionPreset::Standard);
        let comment = meta.to_comment();
        assert!(comment.starts_with(ARCHIVE_COMMENT_HEADER_V3));
        assert_eq!(
            ArchiveVersion::from_comment(comment.as_bytes()),
            ArchiveVersion::V3
        );
        assert_eq!(
            ArchiveMeta::from_comment(comment.as_bytes())
                .unwrap()
                .version,
            3
        );
    }

    #[test]
    fn meta_roundtrip() {
        let meta = ArchiveMeta::new(CompressionPreset::Best);
        let comment = meta.to_comment();
        assert!(comment.starts_with(ARCHIVE_COMMENT_HEADER));

        let parsed = ArchiveMeta::from_comment(comment.as_bytes()).unwrap();
        assert_eq!(parsed.version, 2);
        assert_eq!(parsed.compression, "zstd:19");
    }

    #[test]
    fn meta_from_non_v2_returns_none() {
        assert!(ArchiveMeta::from_comment(b"").is_none());
        assert!(ArchiveMeta::from_comment(V1_COMMENT_MARKER.as_bytes()).is_none());
    }

    #[test]
    fn version_properties() {
        assert!(!ArchiveVersion::Legacy.uses_local_timestamps());
        assert!(!ArchiveVersion::Legacy.uses_save_unit_prefix());

        assert!(ArchiveVersion::V1.uses_local_timestamps());
        assert!(!ArchiveVersion::V1.uses_save_unit_prefix());

        assert!(ArchiveVersion::V2.uses_local_timestamps());
        assert!(ArchiveVersion::V2.uses_save_unit_prefix());
        assert!(ArchiveVersion::V3.uses_local_timestamps());
        assert!(ArchiveVersion::V3.uses_save_unit_prefix());
    }

    #[test]
    fn normalize_entry_path_v2() {
        let v2 = ArchiveVersion::V2;
        assert_eq!(v2.normalize_entry_path("0/save.dat"), Some("save.dat"));
        assert_eq!(
            v2.normalize_entry_path("1/Saves/cfg.ini"),
            Some("Saves/cfg.ini")
        );
        // Pure index directory entries should be skipped
        assert_eq!(v2.normalize_entry_path("0/"), None);
        assert_eq!(v2.normalize_entry_path("0"), None);
    }

    #[test]
    fn normalize_entry_path_legacy_and_v1() {
        for version in [ArchiveVersion::Legacy, ArchiveVersion::V1] {
            assert_eq!(version.normalize_entry_path("save.dat"), Some("save.dat"));
            assert_eq!(
                version.normalize_entry_path("dir/file.txt"),
                Some("dir/file.txt")
            );
        }
    }
}
