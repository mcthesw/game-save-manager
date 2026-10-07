use crate::{backup::GameDraft, config::Config, device::get_current_device_id};

/// Open the exact configured location, or the containing root of a collection.
pub fn open_game_location(
    config: &Config,
    raw: &str,
    game: Option<GameDraft>,
    pattern: bool,
    variables: &super::DeviceVariableEdits,
) -> anyhow::Result<crate::path_launcher::OpenManagedLocationOutcome> {
    let mut config = config.clone();
    super::game_edit::apply_device_variables(&mut config, variables)?;
    let game = game.map(|draft| draft.into_game(None));
    let device = config.devices.get(get_current_device_id());
    let context = game
        .as_ref()
        .map(|game| super::game_path_context(game, device))
        .unwrap_or_else(|| super::device_path_context(device));
    if pattern && !crate::backup::registry::is_registry_path(raw) {
        let path = expression_open_target(raw, &context)?;
        crate::path_launcher::open_path(&path)?;
        return Ok(crate::path_launcher::OpenManagedLocationOutcome::Opened);
    }
    crate::path_launcher::open_managed_location(raw, Some(&context), &config)
}

fn expression_open_target(
    raw: &str,
    context: &crate::path_resolution::ResolutionContext,
) -> anyhow::Result<std::path::PathBuf> {
    let parsed = crate::path_pattern::parse_manifest_path_pattern(raw)?;
    let plan = crate::path_resolution::plan_resolution(&parsed, Default::default(), context);
    if plan.is_blocked() || plan.candidates.len() != 1 {
        anyhow::bail!(
            "{}",
            crate::path_resolution::selection_error(&plan.selection_state)
        );
    }
    let candidate = &plan.candidates[0];
    Ok(candidate
        .exact_target_path()
        .unwrap_or_else(|| candidate.logical_anchor.clone().into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opens_literal_brackets_and_collection_root_without_choosing_a_match() {
        let context = crate::path_resolution::ResolutionContext::default();
        assert_eq!(
            expression_open_target("saves/Game[[]One[]]/save.sav", &context).unwrap(),
            std::path::PathBuf::from("saves/Game[One]/save.sav")
        );
        assert_eq!(
            expression_open_target("saves/*/save.sav", &context).unwrap(),
            std::path::PathBuf::from("saves")
        );
    }
}
