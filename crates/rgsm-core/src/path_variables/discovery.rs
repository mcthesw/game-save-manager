//! Read-only suggestions. Never select a match or change a binding here.
use std::collections::BTreeMap;
use std::path::{Component, PathBuf};
use std::time::{Duration, Instant};

use crate::path_pattern::parse_manifest_path_pattern;
use crate::path_resolution::{ResolutionContext, plan_resolution};

#[derive(Debug, Default, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VariableDiscovery {
    pub candidates: Vec<VariableCandidate>,
    pub missing_variables: Vec<String>,
    pub needs_root: bool,
    pub incomplete: bool,
}
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct VariableCandidate {
    pub value: String,
    pub paths: Vec<String>,
}

/// Infer one path component beneath an already resolved local directory.
/// Root variables are deliberately left to a folder selection or manual input.
pub fn discover_variable(
    paths: &[String],
    name: &str,
    context: &ResolutionContext,
) -> Result<VariableDiscovery, String> {
    if !super::valid_name(name) {
        return Err("invalid variable name".into());
    }
    let token = format!("<var:{name}>");
    let parsed = paths
        .iter()
        .filter(|p| p.contains(&token))
        .map(|p| parse_manifest_path_pattern(p).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let missing_variables = parsed
        .iter()
        .flat_map(|p| &p.variables)
        .filter(|n| {
            n.as_str() != name
                && context
                    .variables
                    .get(*n)
                    .is_none_or(|v| v.trim().is_empty())
        })
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut result = VariableDiscovery {
        missing_variables,
        ..Default::default()
    };
    if !result.missing_variables.is_empty() {
        return Ok(result);
    }
    let mut marker = "RGSMVARIABLECANDIDATE".to_owned();
    while paths.iter().any(|p| p.contains(&marker))
        || context.variables.values().any(|v| v.contains(&marker))
    {
        marker.push('X');
    }
    let mut search_context = context.clone();
    search_context
        .variables
        .insert(name.to_owned(), marker.clone());
    let mut found: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let started = Instant::now();
    let mut inspected = 0;
    for parsed in parsed {
        let plan = plan_resolution(&parsed, Default::default(), &search_context);
        if plan.is_blocked() {
            return Err(plan
                .diagnostics
                .iter()
                .map(|d| d.message.as_str())
                .collect::<Vec<_>>()
                .join("; "));
        }
        for candidate in plan.candidates {
            let expression = &candidate.expression;
            let wildcard = expression.replace(&marker, "*");
            let first = crate::path_resolution::model::first_unescaped_glob(&wildcard).unwrap_or(0);
            let prefix = wildcard[..first]
                .rfind('/')
                .map(|i| &wildcard[..i])
                .unwrap_or("");
            let anchor =
                PathBuf::from(crate::path_resolution::model::unescape_glob_literal(prefix));
            // Never infer a root by scanning a drive, share, or a relative directory.
            if !anchor.is_absolute()
                || !anchor
                    .components()
                    .any(|c| matches!(c, Component::Normal(_)))
                || prefix.starts_with("//")
            {
                result.needs_root = true;
                continue;
            }
            let glob = globset::GlobBuilder::new(expression)
                .literal_separator(true)
                .case_insensitive(!candidate.case_sensitive)
                .build()
                .map_err(|e| e.to_string())?;
            let occurrences = expression.matches(&marker).count();
            let mut pattern = glob.regex().to_string();
            for index in 0..occurrences {
                pattern = pattern.replacen(&marker, &format!("(?P<v{index}>[^/]+)"), 1);
            }
            let matcher = regex::bytes::Regex::new(&pattern).map_err(|e| e.to_string())?;
            if !anchor.exists() {
                continue;
            }
            for entry in walkdir::WalkDir::new(&anchor)
                .follow_links(false)
                .follow_root_links(false)
                .max_depth(12)
            {
                inspected += 1;
                if inspected > 20_000 || started.elapsed() > Duration::from_secs(3) {
                    result.incomplete = true;
                    break;
                }
                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => {
                        result.incomplete = true;
                        continue;
                    }
                };
                if entry.file_type().is_symlink() {
                    continue;
                }
                if entry.depth() == 12 && entry.file_type().is_dir() {
                    result.incomplete = true;
                }
                let path = entry.path().to_string_lossy().replace('\\', "/");
                let Some(captures) = matcher.captures(path.as_bytes()) else {
                    continue;
                };
                let Some(value) = captures
                    .name("v0")
                    .and_then(|v| std::str::from_utf8(v.as_bytes()).ok())
                else {
                    continue;
                };
                if value.contains(['<', '>', '\0']) || value.trim().is_empty() {
                    continue;
                }
                if !(1..occurrences).all(|i| {
                    captures.name(&format!("v{i}")).is_some_and(|v| {
                        if candidate.case_sensitive {
                            v.as_bytes() == value.as_bytes()
                        } else {
                            v.as_bytes().eq_ignore_ascii_case(value.as_bytes())
                        }
                    })
                }) {
                    continue;
                }
                let paths = found.entry(value.to_owned()).or_default();
                if paths.len() < 3 && !paths.contains(&path) {
                    paths.push(path);
                }
                if found.len() >= 100 {
                    result.incomplete = true;
                    break;
                }
            }
            if result.incomplete
                && (inspected > 20_000
                    || started.elapsed() > Duration::from_secs(3)
                    || found.len() >= 100)
            {
                break;
            }
        }
    }
    result.candidates = found
        .into_iter()
        .map(|(value, paths)| VariableCandidate { value, paths })
        .collect();
    Ok(result)
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
