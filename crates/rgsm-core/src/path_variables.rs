//! Single-valued bindings shared by path previews, capture and restore.
use std::collections::BTreeMap;

pub type PathVariables = BTreeMap<String, String>;

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

pub fn validate(values: &PathVariables) -> Result<(), String> {
    for (name, value) in values {
        if !valid_name(name) || value.trim().is_empty() || value.contains(['\0', '<', '>']) {
            return Err(rust_i18n::t!("path_variables.invalid_value", name = name).to_string());
        }
    }
    Ok(())
}

pub fn effective(defaults: &PathVariables, overrides: Option<&PathVariables>) -> PathVariables {
    let mut values = defaults.clone();
    if let Some(overrides) = overrides {
        values.extend(overrides.clone());
    }
    values
}

pub fn expand(raw: &str, names: &[String], values: &PathVariables) -> Result<String, String> {
    let mut expression = raw.to_string();
    for name in names {
        let value = values
            .get(name)
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| rust_i18n::t!("path_variables.missing", name = name).to_string())?;
        expression = expression.replace(
            &format!("<var:{name}>"),
            &globset::escape(&value.replace('\\', "/")),
        );
    }
    Ok(expression)
}

mod capture;
pub use capture::{VariablePattern, capture_relative_expression, restore_relative_expression};
