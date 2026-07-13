//! Filesystem-safe storage key generation.
//!
//! A *storage key* is a stable, cross-platform-safe identifier derived from a
//! game's display name. It is used as the directory name for local backups
//! and as the path segment in remote cloud storage, so it must never contain
//! characters that are illegal on any supported OS.

use std::collections::HashSet;

/// Characters forbidden in file/directory names on Windows.
const ILLEGAL_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Windows reserved device names (case-insensitive).
/// These are reserved both bare (`CON`) and with any extension (`CON.txt`).
const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
    "COM8", "COM9", "COM¹", "COM²", "COM³", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6",
    "LPT7", "LPT8", "LPT9", "LPT¹", "LPT²", "LPT³",
];

/// Maximum length for a storage key.
///
/// Snapshot filenames are `{date}.7z` or historical `{date}.zip` appended inside the
/// directory, and most filesystems cap path *components* at 255 bytes.
/// 200 chars leaves ample room.
const MAX_KEY_LEN: usize = 200;

/// Derive a filesystem-safe storage key from a display name.
///
/// The rules (applied in order):
/// 1. Replace each illegal character with `_`.
/// 2. Strip ASCII control characters (0x00–0x1F, 0x7F).
/// 3. Trim trailing dots and spaces (Windows restriction).
/// 4. Collapse consecutive underscores into one.
/// 5. If the result equals `.` or `..`, or its stem (before the first `.`)
///    matches a Windows reserved device name (case-insensitive), append `_`
///    to disambiguate.
/// 6. If the result is empty, return `_unnamed`.
/// 7. Truncate to [`MAX_KEY_LEN`] characters.
pub fn generate_storage_key(display_name: &str) -> String {
    // Step 1 & 2: replace illegal chars and strip control chars
    let mut key: String = display_name
        .chars()
        .filter(|c| !c.is_ascii_control())
        .map(|c| if ILLEGAL_CHARS.contains(&c) { '_' } else { c })
        .collect();

    // Step 3: trim trailing dots and spaces
    let trimmed_len = key.trim_end_matches(['.', ' ']).len();
    key.truncate(trimmed_len);

    // Step 4: collapse consecutive underscores
    let mut collapsed = String::with_capacity(key.len());
    let mut prev_was_underscore = false;
    for c in key.chars() {
        if c == '_' {
            if !prev_was_underscore {
                collapsed.push('_');
            }
            prev_was_underscore = true;
        } else {
            collapsed.push(c);
            prev_was_underscore = false;
        }
    }
    key = collapsed;

    // Step 5: handle "." / ".." and reserved names (bare or with extension)
    if key == "." || key == ".." || is_reserved_stem(&key) {
        key.push('_');
    }

    // Step 6: empty → fallback
    if key.is_empty() {
        key = "_unnamed".to_string();
    }

    // Step 7: truncate (char-boundary safe)
    if key.len() > MAX_KEY_LEN {
        let mut end = MAX_KEY_LEN;
        while !key.is_char_boundary(end) {
            end -= 1;
        }
        key.truncate(end);
    }

    key
}

/// Check if the *stem* (part before the first `.`) matches a Windows reserved
/// device name. E.g. `CON`, `CON.txt`, `nul.save` are all reserved.
fn is_reserved_stem(name: &str) -> bool {
    let stem = match name.find('.') {
        Some(pos) => &name[..pos],
        None => name,
    };
    RESERVED_NAMES.iter().any(|r| r.eq_ignore_ascii_case(stem))
}

/// Generate a storage key that is unique among `existing_keys`.
///
/// Comparison is **case-insensitive** to avoid collisions on
/// case-insensitive filesystems (Windows, macOS default).
/// If the base key already exists, appends `_2`, `_3`, … until a free
/// slot is found.
pub fn generate_unique_storage_key(display_name: &str, existing_keys: &HashSet<String>) -> String {
    let base = generate_storage_key(display_name);
    if !existing_keys.iter().any(|k| k.eq_ignore_ascii_case(&base)) {
        return base;
    }

    let mut suffix = 2u32;
    loop {
        let candidate = format!("{base}_{suffix}");
        if !existing_keys
            .iter()
            .any(|k| k.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
        suffix += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_sanitization() {
        assert_eq!(
            generate_storage_key("Fallout: New Vegas"),
            "Fallout_ New Vegas"
        );
    }

    #[test]
    fn multiple_illegal_chars() {
        assert_eq!(
            generate_storage_key(r#"Game: "Demo" <Test>"#),
            "Game_ _Demo_ _Test_"
        );
    }

    #[test]
    fn collapses_consecutive_underscores() {
        assert_eq!(generate_storage_key("A:::B"), "A_B");
    }

    #[test]
    fn strips_trailing_dots_and_spaces() {
        assert_eq!(generate_storage_key("Game..."), "Game");
        assert_eq!(generate_storage_key("Game   "), "Game");
        assert_eq!(generate_storage_key("Game. . ."), "Game");
    }

    #[test]
    fn empty_name_produces_unnamed() {
        assert_eq!(generate_storage_key(""), "_unnamed");
        assert_eq!(generate_storage_key("..."), "_unnamed");
        // ":::" → "___" → collapse → "_"
        assert_eq!(generate_storage_key(":::"), "_");
    }

    #[test]
    fn reserved_names_get_suffix() {
        assert_eq!(generate_storage_key("CON"), "CON_");
        assert_eq!(generate_storage_key("con"), "con_");
        assert_eq!(generate_storage_key("NUL"), "NUL_");
        assert_eq!(generate_storage_key("LPT1"), "LPT1_");
    }

    #[test]
    fn reserved_names_with_extension() {
        assert_eq!(generate_storage_key("CON.txt"), "CON.txt_");
        assert_eq!(generate_storage_key("nul.save"), "nul.save_");
        assert_eq!(generate_storage_key("LPT1.dat"), "LPT1.dat_");
    }

    #[test]
    fn dot_and_dotdot() {
        // "." → trimmed trailing dots → "" → "_unnamed"
        assert_eq!(generate_storage_key("."), "_unnamed");
        // ".." → trimmed trailing dots → "" → "_unnamed"
        assert_eq!(generate_storage_key(".."), "_unnamed");
    }

    #[test]
    fn normal_name_unchanged() {
        assert_eq!(generate_storage_key("Elden Ring"), "Elden Ring");
        assert_eq!(generate_storage_key("ゼルダの伝説"), "ゼルダの伝説");
    }

    #[test]
    fn strips_control_chars() {
        assert_eq!(generate_storage_key("Game\x00Name"), "GameName");
        assert_eq!(generate_storage_key("Game\x1FName"), "GameName");
    }

    #[test]
    fn truncates_long_names() {
        let long_name = "A".repeat(300);
        let key = generate_storage_key(&long_name);
        assert!(key.len() <= MAX_KEY_LEN);
        assert_eq!(key.len(), MAX_KEY_LEN);
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        // 'é' is 2 bytes in UTF-8
        let name = "é".repeat(150);
        let key = generate_storage_key(&name);
        assert!(key.len() <= MAX_KEY_LEN);
        // Must still be valid UTF-8 (this is enforced by String type)
        assert!(key.is_char_boundary(key.len()));
    }

    #[test]
    fn unique_key_no_collision() {
        let existing = HashSet::new();
        assert_eq!(
            generate_unique_storage_key("Test Game", &existing),
            "Test Game"
        );
    }

    #[test]
    fn unique_key_with_collision() {
        let mut existing = HashSet::new();
        existing.insert("Test Game".to_string());
        assert_eq!(
            generate_unique_storage_key("Test Game", &existing),
            "Test Game_2"
        );
    }

    #[test]
    fn unique_key_case_insensitive_collision() {
        let mut existing = HashSet::new();
        existing.insert("test game".to_string());
        assert_eq!(
            generate_unique_storage_key("Test Game", &existing),
            "Test Game_2"
        );
    }

    #[test]
    fn unique_key_multiple_collisions() {
        let mut existing = HashSet::new();
        existing.insert("Test Game".to_string());
        existing.insert("Test Game_2".to_string());
        existing.insert("Test Game_3".to_string());
        assert_eq!(
            generate_unique_storage_key("Test Game", &existing),
            "Test Game_4"
        );
    }

    #[test]
    fn different_illegal_chars_same_base_key() {
        // Games that differ only by illegal chars should produce the same base key
        let key1 = generate_storage_key("Game: Demo");
        let key2 = generate_storage_key("Game| Demo");
        assert_eq!(key1, key2);

        // But unique generation handles this
        let mut existing = HashSet::new();
        existing.insert(key1);
        let key2_unique = generate_unique_storage_key("Game| Demo", &existing);
        assert_eq!(key2_unique, "Game_ Demo_2");
    }
}
