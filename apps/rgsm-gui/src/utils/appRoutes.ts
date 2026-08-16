const KNOWN_APP_PAGES: Record<string, true> = {
  '/': true,
  '/About': true,
  '/Settings': true,
  '/SyncSettings': true,
};

const LEGACY_HOME_PAGES: Record<string, string> = {
  '/AddGame': '/',
};

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
  games: readonly { name: string }[],
  routeName: string | string[] | undefined
): boolean {
  const gameName = getGameNameFromRouteParam(routeName);
  return gameName.length > 0 && games.some((game) => game.name === gameName);
}

export function isValidAppDestination(
  path: string,
  games: readonly { name: string }[],
  routeName?: string | string[]
): boolean {
  const mapped = mapLegacyHomePage(path);
  if (KNOWN_APP_PAGES[mapped]) return true;
  if (!mapped.startsWith('/Management')) return false;
  const name = routeName ?? mapped.slice('/Management/'.length);
  return managementGameExists(games, name);
}

export function resolveStartupDestination(
  currentPath: string,
  homePage: string | null | undefined,
  games: readonly { name: string }[],
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
