use super::ServiceContext;
use crate::{
    backup::{Game, GameDraft},
    config::Config,
    device::get_current_device_id,
    hooks::{GameAddedCtx, GameUpdatedCtx, HookSource},
};
use anyhow::{Result, bail};
use std::collections::BTreeMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeviceVariableEdit {
    pub value: String,
    pub previous_value: Option<String>,
}
pub type DeviceVariableEdits = BTreeMap<String, DeviceVariableEdit>;

pub(crate) fn apply_device_variables(
    config: &mut Config,
    edits: &DeviceVariableEdits,
) -> Result<()> {
    if edits.is_empty() {
        return Ok(());
    }
    crate::path_variables::validate(
        &edits
            .iter()
            .map(|(name, edit)| (name.clone(), edit.value.clone()))
            .collect(),
    )
    .map_err(anyhow::Error::msg)?;
    let device = config
        .devices
        .entry(get_current_device_id().clone())
        .or_default();
    for (name, edit) in edits {
        if device.path_variables.get(name) != edit.previous_value.as_ref() {
            bail!("{}", rust_i18n::t!("path_variables.changed", name = name));
        }
    }
    device.path_variables.extend(
        edits
            .iter()
            .map(|(name, edit)| (name.clone(), edit.value.clone())),
    );
    Ok(())
}

impl ServiceContext {
    pub async fn save_game_edit(
        &self,
        storage_key: Option<&str>,
        draft: &GameDraft,
        variables: &DeviceVariableEdits,
        source: HookSource,
    ) -> Result<Game> {
        let ((previous, game), config) = crate::config::edit_config(|config| {
            apply_device_variables(config, variables)?;
            let index = storage_key
                .map(|key| {
                    config
                        .games
                        .iter()
                        .position(|g| g.storage_key == key)
                        .ok_or_else(|| anyhow::anyhow!("Game '{key}' not found"))
                })
                .transpose()?;
            if config
                .games
                .iter()
                .enumerate()
                .any(|(i, g)| Some(i) != index && g.name.eq_ignore_ascii_case(&draft.name))
            {
                bail!("Game '{}' already exists", draft.name);
            }
            config.bind_legacy_game_references();
            let previous = index.map(|i| config.games[i].clone());
            let game = draft.clone().into_game(previous.as_ref());
            self.validate_game_paths(config, &game)?;
            let saved = if let Some(index) = index {
                config.games[index] = game.clone();
                let old = previous.as_ref().expect("existing game");
                config.quick_action.sync_updated_game_reference(old, &game);
                crate::config::FavoriteTreeNode::rename_game_leaves(
                    &mut config.favorites,
                    &old.storage_key,
                    &game.name,
                );
                game
            } else {
                crate::backup::create_game_backup(draft, config)?
            };
            Ok((previous, saved))
        })?;
        if let Some(previous_game) = previous {
            self.pipeline()
                .fire_game_updated(&GameUpdatedCtx {
                    config,
                    source,
                    previous_game,
                    game: game.clone(),
                })
                .await;
        } else {
            let snapshots = game.get_game_snapshots_info()?;
            self.pipeline()
                .fire_game_added(&GameAddedCtx {
                    config,
                    source,
                    game: game.clone(),
                    snapshots,
                })
                .await;
        }
        Ok(game)
    }
}

#[cfg(test)]
#[path = "game_edit_tests.rs"]
mod tests;
