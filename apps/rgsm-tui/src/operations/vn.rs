use anyhow::Result;
use rgsm_core::{
    backup::GameDraft, config::get_config, hooks::HookSource, services::ServiceContext,
};

pub(super) async fn import_vn_games(
    service: &ServiceContext,
    drafts: Vec<GameDraft>,
) -> Result<usize> {
    let mut known_names = get_config()?
        .games
        .iter()
        .map(|game| game.name.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let mut imported = 0usize;

    for draft in drafts {
        let name = draft.name.trim();
        if name.is_empty()
            || known_names
                .iter()
                .any(|known| known == &name.to_ascii_lowercase())
        {
            continue;
        }
        service.add_game(&draft, HookSource::UserManual).await?;
        known_names.push(name.to_ascii_lowercase());
        imported += 1;
    }

    Ok(imported)
}
