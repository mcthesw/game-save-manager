use std::collections::BTreeSet;

use crate::{
    backup::{Game, SaveUnitSource},
    config::Config,
};

/// The same parsed variable references and precedence used by capture and restore.
/// Only enabled save locations are required to continue a save operation.
pub fn missing_game_variables(config: &Config, game: &Game, device_id: &str) -> Vec<String> {
    let defaults = config
        .devices
        .get(device_id)
        .map(|d| &d.path_variables)
        .cloned()
        .unwrap_or_default();
    let values = crate::path_variables::effective(
        &defaults,
        game.device_bindings
            .get(device_id)
            .map(|b| &b.path_variables),
    );
    game.save_paths
        .iter()
        .filter(|u| u.enabled)
        .filter_map(|unit| {
            game.path_override(unit.id, &device_id.to_owned())
                .map(|p| p.path.as_str())
                .or_else(|| match &unit.source {
                    SaveUnitSource::Concrete { paths, .. } => {
                        paths.get(device_id).map(String::as_str)
                    }
                    SaveUnitSource::ManifestPattern { pattern, .. } => Some(pattern.raw()),
                })
        })
        .filter_map(|path| crate::path_pattern::parse_manifest_path_pattern(path).ok())
        .flat_map(|parsed| parsed.variables)
        .filter(|name| values.get(name).is_none_or(|value| value.trim().is_empty()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_variables_respect_device_scope_overrides_and_disabled_units() {
        let mut config = Config::default();
        config.devices.insert(
            "local".into(),
            serde_json::from_value(serde_json::json!({
                "id":"local", "name":"Local", "path_variables":{"root":"/saves"}
            }))
            .unwrap(),
        );
        let mut game: Game = serde_json::from_value(serde_json::json!({
            "name":"Game", "save_paths":[
                {"id":0,"source":{"type":"manifestPattern","pattern":"<var:root>/<var:account>/*.sav"}},
                {"id":1,"enabled":false,"source":{"type":"manifestPattern","pattern":"<var:disabled>"}}
            ], "device_bindings":{"other":{"pathVariables":{"account":"other-account"}}}
        })).unwrap();
        assert_eq!(
            missing_game_variables(&config, &game, "local"),
            vec!["account"]
        );
        game.device_bindings
            .entry("local".into())
            .or_default()
            .path_variables
            .insert("account".into(), "new-account".into());
        assert!(missing_game_variables(&config, &game, "local").is_empty());
        assert_eq!(
            missing_game_variables(&config, &game, "other"),
            vec!["root"]
        );
    }
}
