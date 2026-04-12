export function getGameManagementPath(gameName: string): string {
  return `/Management/${encodeURIComponent(gameName)}`;
}

export function getGameNameFromRouteParam(routeName: string | string[] | undefined): string {
  const raw = Array.isArray(routeName) ? (routeName[0] ?? '') : (routeName ?? '');

  try {
    return decodeURIComponent(raw);
  } catch {
    return raw;
  }
}
