import type { Device, Game } from '../api/commands';

export function gameRootPathKey(path: string): string {
  const windowsPath =
    /^[a-z]:[\\/]/i.test(path) || path.startsWith('\\\\') || path.startsWith('//');
  const normalized = windowsPath ? path.replaceAll('\\', '/').toLowerCase() : path;
  const key = normalized.replace(/\/+$/, '') || '/';
  return windowsPath && /^[a-z]:$/i.test(key) ? `${key}/` : key;
}

export function newGameRootPaths(existing: string[], detected: string[]): string[] {
  const paths = new Set(existing.map(gameRootPathKey));
  return detected.filter((path) => {
    const key = gameRootPathKey(path);
    if (paths.has(key)) return false;
    paths.add(key);
    return true;
  });
}

/** Keep stable resource IDs and retarget bindings before dropping duplicate roots. */
export function mergeDuplicateGameRoots(device: Device, games: Game[]): void {
  const roots = new Map<string, number>();
  const replacements = new Map<number, number>();
  device.resources = (device.resources ?? []).filter((resource) => {
    if (resource.kind.type !== 'gameRoot' || !resource.kind.path.trim()) return true;
    const key = `${resource.kind.store}:${gameRootPathKey(resource.kind.path)}`;
    const retained = roots.get(key);
    if (retained === undefined) {
      roots.set(key, resource.id);
      return true;
    }
    replacements.set(resource.id, retained);
    return false;
  });
  for (const resource of device.resources) {
    if (resource.kind.type === 'gameInstallation') {
      resource.kind.root_id = replacements.get(resource.kind.root_id) ?? resource.kind.root_id;
    }
  }
  for (const game of games) {
    const binding = game.device_bindings?.[device.id];
    if (binding?.rootIds) {
      binding.rootIds = [...new Set(binding.rootIds.map((id) => replacements.get(id) ?? id))];
    }
  }
}
