use crate::backup::Game;

/// Prefer a stable key. A legacy name is usable only when it identifies one game.
pub(super) fn position_game_by_identity(games: &[Game], identity: &str) -> Option<usize> {
    if identity.is_empty() {
        return None;
    }
    if let Some(index) = games.iter().position(|game| game.storage_key == identity) {
        return Some(index);
    }
    let mut matches = games
        .iter()
        .enumerate()
        .filter(|(_, game)| game.name == identity);
    let (index, _) = matches.next()?;
    matches.next().is_none().then_some(index)
}
