import {
  commands,
  type CandidateDimensions,
  type DeviceResource,
  type Game,
  type SaveUnit,
} from '~/bindings';

export function usePathResolution() {
  async function preview(game: Game, unit: SaveUnit) {
    const result = await commands.previewSaveUnitResolution(game, unit);
    return result.status === 'ok' ? result.data : null;
  }

  async function rememberRestoreMapping(
    game: Game,
    saveUnitId: number,
    sourceDimensions: CandidateDimensions,
    targetCandidateIds: string[]
  ) {
    return commands.saveRestoreMapping(
      game.storage_key || game.name,
      saveUnitId,
      sourceDimensions,
      targetCandidateIds
    );
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

  return { preview, rememberRestoreMapping, resourceLabel };
}
