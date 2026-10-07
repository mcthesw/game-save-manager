type NamedGame = { name: string };

export function nextInstanceName(games: readonly NamedGame[], title: string): string {
  const names = new Set(games.map((game) => game.name.toLowerCase()));
  let suffix = 2;
  while (names.has(`${title} (${suffix})`.toLowerCase())) suffix++;
  return `${title} (${suffix})`;
}

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
