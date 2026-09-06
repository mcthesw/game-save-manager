type NamedGame = { name: string };

/** Existing same-title games remain editable; only a new or changed name can conflict. */
export function hasGameNameConflict(
  games: readonly NamedGame[],
  nextName: string,
  existing?: NamedGame | null
): boolean {
  const name = nextName.trim().toLowerCase();
  if (existing?.name.toLowerCase() === name) return false;
  return games.some((game) => game.name.toLowerCase() === name);
}
