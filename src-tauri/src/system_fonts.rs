//! System font enumeration module
//!
//! Provides cross-platform functionality to list installed fonts on the system
//! using font-kit which leverages native system APIs (DirectWrite on Windows,
//! Core Text on macOS, fontconfig on Linux).

use font_kit::source::SystemSource;
use log::{debug, warn};

/// Get a sorted list of unique font family names installed on the system.
///
/// Uses the system's native font API:
/// - Windows: DirectWrite
/// - macOS: Core Text
/// - Linux: fontconfig
pub fn get_system_fonts() -> Vec<String> {
    let source = SystemSource::new();

    match source.all_families() {
        Ok(mut families) => {
            debug!(target: "rgsm::fonts", "Found {} system font families", families.len());
            families.sort();
            families
        }
        Err(e) => {
            warn!(target: "rgsm::fonts", "Failed to enumerate system fonts: {:?}", e);
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_system_fonts() {
        let fonts = get_system_fonts();
        // Should return at least some fonts on any desktop system
        assert!(
            !fonts.is_empty(),
            "Should find at least one font on the system"
        );

        // Fonts should be sorted
        let mut sorted = fonts.clone();
        sorted.sort();
        assert_eq!(fonts, sorted, "Fonts should be sorted alphabetically");
    }
}
