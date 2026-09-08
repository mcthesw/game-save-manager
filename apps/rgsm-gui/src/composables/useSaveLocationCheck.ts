import { commands, type SaveUnit, type SaveUnitDraft } from '../api/commands';
import { $t } from '../i18n';
import { notifyWarning } from './useActivityCenter';

/** Availability is a warning, not a condition for saving device configuration. */
export function useSaveLocationCheck() {
  async function warnUnavailableLocations(
    units: (SaveUnit | SaveUnitDraft)[],
    deviceId: string
  ): Promise<void> {
    const paths = units.flatMap((unit) => {
      if (unit.enabled === false || unit.source.type !== 'concrete') return [];
      const path = unit.source.paths?.[deviceId]?.trim();
      return path ? [path] : [];
    });
    if (!paths.length) return;
    try {
      const result = await commands.checkPaths(paths, null, null, null);
      if (result.status === 'error') {
        notifyWarning($t('path_variable.check_unavailable'));
        return;
      }
      const details = result.data.flatMap((check) => {
        if (check.status === 'ok') return [];
        if (check.status === 'registryPath' && check.supported && check.exists) return [];
        const reason =
          check.status === 'resolveFailed'
            ? check.error
            : check.status === 'registryPath' && !check.supported
              ? $t('path_variable.registry_not_supported')
              : $t('path_variable.not_found');
        return [`${check.rawPath}: ${reason}`];
      });
      if (details.length) notifyWarning($t('path_variable.saved_unavailable'), details.join('\n'));
    } catch {
      notifyWarning($t('path_variable.check_unavailable'));
    }
  }
  return { warnUnavailableLocations };
}
