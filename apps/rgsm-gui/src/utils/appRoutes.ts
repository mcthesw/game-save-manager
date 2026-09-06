const KNOWN_APP_PAGES: Record<string, true> = {
  '/': true,
  '/About': true,
  '/Settings': true,
  '/SyncSettings': true,
};

const LEGACY_HOME_PAGES: Record<string, string> = {
  '/AddGame': '/',
};

type NamedGame = { name: string; storage_key?: string };

/** An explicit ID is authoritative; only unbound legacy references use a unique name. */
export function resolveGameReference<T extends NamedGame>(
  games: readonly T[],
  name: string,
  gameId?: string | null
): T | undefined {
  if (gameId != null) return games.find((game) => gameId !== '' && game.storage_key === gameId);
  const matches = games.filter((game) => name !== '' && game.name === name);
  return matches.length === 1 ? matches[0] : undefined;
}

export function getGameManagementPath(game: NamedGame): string {
  const path = `/Management/${encodeURIComponent(game.name)}`;
  return game.storage_key ? `${path}?gameId=${encodeURIComponent(game.storage_key)}` : path;
}

export function resolveManagementGame<T extends NamedGame>(
  games: readonly T[],
  path: string,
  routeName?: string | string[]
): T | undefined {
  if (!path.startsWith('/Management/')) return;
  const url = new URL(path, 'http://app.invalid');
  if (!/^\/Management\/[^/]+$/.test(url.pathname)) return;
  return resolveGameReference(
    games,
    getGameNameFromRouteParam(routeName ?? url.pathname.slice('/Management/'.length)),
    url.searchParams.get('gameId')
  );
}

export function getGameNameFromRouteParam(routeName: string | string[] | undefined): string {
  const raw = Array.isArray(routeName) ? (routeName[0] ?? '') : (routeName ?? '');
  try {
    return decodeURIComponent(raw);
  } catch {
    return raw;
  }
}

export function mapLegacyHomePage(path: string | null | undefined): string {
  const raw = path?.trim() ? path : '/';
  return LEGACY_HOME_PAGES[raw] ?? raw;
}

export function managementGameExists(
  games: readonly NamedGame[],
  routeName: string | string[] | undefined
): boolean {
  const gameName = getGameNameFromRouteParam(routeName);
  return resolveGameReference(games, gameName) !== undefined;
}

export function isValidAppDestination(
  path: string,
  games: readonly NamedGame[],
  routeName?: string | string[]
): boolean {
  const mapped = mapLegacyHomePage(path);
  if (KNOWN_APP_PAGES[mapped]) return true;
  return resolveManagementGame(games, mapped, routeName) !== undefined;
}

export function resolveStartupDestination(
  currentPath: string,
  homePage: string | null | undefined,
  games: readonly NamedGame[],
  routeName?: string | string[]
): string {
  const mappedCurrent = mapLegacyHomePage(currentPath);
  if (mappedCurrent.startsWith('/Management')) {
    return isValidAppDestination(mappedCurrent, games, routeName) ? mappedCurrent : '/';
  }
  if (mappedCurrent !== '/' && KNOWN_APP_PAGES[mappedCurrent]) {
    return mappedCurrent;
  }

  const mappedHome = mapLegacyHomePage(homePage);
  if (isValidAppDestination(mappedHome, games)) {
    return mappedHome;
  }
  return '/';
}
