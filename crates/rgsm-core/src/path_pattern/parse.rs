use thiserror::Error;

use super::{ManifestPathPattern, ParsedManifestPathPattern, PathPlaceholder};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PathPatternError {
    #[error("path pattern is empty")]
    Empty,
    #[error("incomplete path placeholder starting at byte {offset}")]
    IncompletePlaceholder { offset: usize },
    #[error("unexpected closing placeholder delimiter at byte {offset}")]
    UnexpectedClosingDelimiter { offset: usize },
    #[error("unknown path placeholder: {token}")]
    UnknownPlaceholder { token: String },
    #[error("invalid glob syntax: {message}")]
    InvalidGlob { message: String },
}

pub fn is_dynamic_manifest_path(raw: &str) -> bool {
    raw.contains('<') || raw.contains('*') || raw.contains('?')
}

pub fn parse_manifest_path_pattern(
    raw: impl Into<String>,
) -> Result<ParsedManifestPathPattern, PathPatternError> {
    let raw = raw.into();
    if raw.trim().is_empty() {
        return Err(PathPatternError::Empty);
    }

    let mut placeholders = Vec::new();
    let mut cursor = 0;
    while cursor < raw.len() {
        let remaining = &raw[cursor..];
        let next_open = remaining.find('<').map(|offset| cursor + offset);
        let next_close = remaining.find('>').map(|offset| cursor + offset);

        if let Some(close) = next_close
            && next_open.is_none_or(|open| close < open)
        {
            return Err(PathPatternError::UnexpectedClosingDelimiter { offset: close });
        }

        let Some(open) = next_open else {
            break;
        };
        let Some(relative_close) = raw[open + 1..].find('>') else {
            return Err(PathPatternError::IncompletePlaceholder { offset: open });
        };
        let close = open + 1 + relative_close;
        let token = &raw[open..=close];
        let placeholder = PathPlaceholder::from_token(token).ok_or_else(|| {
            PathPatternError::UnknownPlaceholder {
                token: token.to_string(),
            }
        })?;
        if !placeholders.contains(&placeholder) {
            placeholders.push(placeholder);
        }
        cursor = close + 1;
    }

    globset::Glob::new(&raw.replace('\\', "/")).map_err(|error| PathPatternError::InvalidGlob {
        message: error.to_string(),
    })?;

    Ok(ParsedManifestPathPattern {
        pattern: ManifestPathPattern::new(raw),
        placeholders,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_canonical_placeholders() {
        let raw = PathPlaceholder::ALL
            .into_iter()
            .map(PathPlaceholder::token)
            .collect::<Vec<_>>()
            .join("/");

        let parsed = parse_manifest_path_pattern(raw).unwrap();

        assert_eq!(parsed.placeholders, PathPlaceholder::ALL);
    }

    #[test]
    fn accepts_legacy_store_user_id_alias_without_rewriting_raw_input() {
        let parsed = parse_manifest_path_pattern("<root>/userdata/<storeuserid>/*.sav").unwrap();

        assert_eq!(parsed.pattern.raw(), "<root>/userdata/<storeuserid>/*.sav");
        assert_eq!(
            parsed.placeholders,
            vec![PathPlaceholder::Root, PathPlaceholder::StoreUserId]
        );
    }

    #[test]
    fn reports_unknown_and_incomplete_placeholders() {
        assert!(matches!(
            parse_manifest_path_pattern("<unknown>/save"),
            Err(PathPatternError::UnknownPlaceholder { .. })
        ));
        assert!(matches!(
            parse_manifest_path_pattern("<home/save"),
            Err(PathPatternError::IncompletePlaceholder { .. })
        ));
    }

    #[test]
    fn reports_invalid_glob_syntax() {
        assert!(matches!(
            parse_manifest_path_pattern("<home>/save/[abc"),
            Err(PathPatternError::InvalidGlob { .. })
        ));
    }

    #[test]
    fn literal_brackets_alone_do_not_make_a_legacy_path_dynamic() {
        assert!(!is_dynamic_manifest_path("C:/Games/Game[One]/save.dat"));
    }
}
