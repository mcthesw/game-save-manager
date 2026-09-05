type IdentifiedGame = { storage_key?: string; name: string };

/** Apply a draft order to current definitions, dropping removed games and appending new ones. */
export function applyGameOrder<T extends IdentifiedGame>(games: T[], order: string[]): T[] {
  const remaining = new Map(games.map((game) => [game.storage_key || game.name, game]));
  const ordered: T[] = [];
  for (const id of order) {
    const game = remaining.get(id);
    if (game) ordered.push(game);
    remaining.delete(id);
  }
  return [...ordered, ...remaining.values()];
}
