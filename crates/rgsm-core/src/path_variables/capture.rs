//! Preserve variable occurrences after globs without reinterpreting matched names.
use super::{PathVariables, expand};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, utoipa::ToSchema)]
pub struct VariablePattern {
    pub expression: String,
    pub values: std::collections::BTreeMap<String, String>,
}

pub fn capture_relative_expression(
    pattern: &VariablePattern,
    path: &str,
    anchor: &str,
    case_sensitive: bool,
) -> Result<Option<String>, String> {
    let path = path.replace('\\', "/");
    let anchor = anchor.replace('\\', "/");
    let offset = if anchor == "." {
        0
    } else {
        anchor.trim_end_matches('/').len() + 1
    };
    if offset > path.len() {
        return Ok(None);
    }
    let mut marked = pattern.expression.clone();
    let mut prefix = "RGSMVAR".to_string();
    while marked.contains(&prefix) {
        prefix.push('X');
    }
    let mut occurrences = Vec::new();
    for (name, value) in &pattern.values {
        let token = format!("<var:{name}>");
        while marked.contains(&token) {
            let marker = format!("{prefix}{}END", occurrences.len());
            marked = marked.replacen(&token, &marker, 1);
            occurrences.push((name, value.replace('\\', "/"), marker));
        }
    }
    let glob = globset::GlobBuilder::new(&marked)
        .literal_separator(true)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| e.to_string())?;
    let mut regex = glob.regex().to_string();
    for (i, (_, value, marker)) in occurrences.iter().enumerate() {
        regex = regex.replace(marker, &format!("(?P<v{i}>{})", regex::escape(value)));
    }
    let regex = regex::bytes::Regex::new(&regex).map_err(|e| e.to_string())?;
    let captures = regex
        .captures(path.as_bytes())
        .ok_or("cannot preserve variable positions in matched path")?;
    let mut spans = occurrences
        .iter()
        .enumerate()
        .filter_map(|(i, (name, _, _))| {
            captures
                .name(&format!("v{i}"))
                .filter(|m| m.start() >= offset)
                .map(|m| (m.start(), m.end(), name))
        })
        .collect::<Vec<_>>();
    if spans.is_empty() {
        return Ok(None);
    }
    spans.sort_by_key(|s| s.0);
    let mut result = String::new();
    let mut cursor = offset;
    for (start, end, name) in spans {
        result.push_str(&globset::escape(&path[cursor..start]));
        result.push_str(&format!("<var:{name}>"));
        cursor = end;
    }
    result.push_str(&globset::escape(&path[cursor..]));
    Ok(Some(result))
}

pub fn restore_relative_expression(
    expression: &str,
    values: &PathVariables,
) -> Result<std::path::PathBuf, String> {
    let parsed =
        crate::path_pattern::parse_manifest_path_pattern(expression).map_err(|e| e.to_string())?;
    let expanded = expand(expression, &parsed.variables, values)?;
    let path = std::path::PathBuf::from(crate::path_resolution::model::unescape_glob_literal(
        &expanded,
    ));
    if path.components().any(|c| {
        !matches!(
            c,
            std::path::Component::Normal(_) | std::path::Component::CurDir
        )
    }) {
        return Err("variable restore target must remain inside its configured root".into());
    }
    Ok(path)
}
