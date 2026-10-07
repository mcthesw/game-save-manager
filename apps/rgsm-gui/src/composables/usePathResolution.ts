import { commands, type DeviceResource, type Game, type SaveUnit } from '~/api/commands';

export function usePathResolution() {
  async function preview(game: Game, unit: SaveUnit) {
    const result = await commands.previewSaveUnitResolution(game, unit);
    return result.status === 'ok' ? result.data : null;
  }

  function resourceLabel(resource: DeviceResource): string {
    switch (resource.kind.type) {
      case 'gameRoot':
        return `${resource.kind.store} · ${resource.kind.path}`;
      case 'storeAccount':
        return `${resource.kind.store} · ${resource.kind.user_id}`;
      case 'gameInstallation':
        return `${resource.kind.store} · ${resource.kind.install_dir}`;
    }
  }

  return { preview, resourceLabel };
}
