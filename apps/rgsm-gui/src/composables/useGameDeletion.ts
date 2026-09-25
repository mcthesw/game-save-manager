import { Trash2 } from '@lucide/vue';
import { commands } from '../api/commands';
import { $t } from '../i18n';
import { type KMenuEntry } from '../ui/kit';
import { useConfig } from './useConfig';
import { useFeedback } from './useFeedback';
import { notifyError, notifySuccess } from './useActivityCenter';

export interface DeletionTarget {
  id: string;
  name: string;
  shared: boolean;
  hasLocalData: boolean;
}

export function useGameDeletion() {
  const { config, refreshConfig } = useConfig();
  const feedback = useFeedback();

  function entries(game: DeletionTarget): KMenuEntry[] {
    return [
      ...(game.hasLocalData ? ['deleteLocal'] : []),
      ...(game.shared ? ['deleteEverywhere'] : []),
    ].map((key) => ({
      type: 'item',
      key,
      icon: Trash2,
      danger: true,
      label: $t(key === 'deleteLocal' ? 'manage.delete_local' : 'manage.delete_everywhere'),
    }));
  }

  async function remove(game: DeletionTarget, key: string): Promise<boolean> {
    const everywhere = key === 'deleteEverywhere';
    if (!entries(game).some((entry) => entry.type === 'item' && entry.key === key)) return false;
    const label = $t(everywhere ? 'manage.delete_everywhere' : 'manage.delete_local');
    try {
      await feedback.prompt(
        $t(everywhere ? 'manage.delete_everywhere_confirm' : 'manage.delete_local_confirm', {
          game: game.name,
        }),
        label,
        {
          type: 'error',
          confirmButtonText: label,
          cancelButtonText: $t('manage.cancel'),
          inputPattern: /^yes$/,
          inputErrorMessage: $t('manage.delete_game_confirmation_error'),
        }
      );
    } catch {
      return false;
    }
    const local = config.value.games.find((item) => (item.storage_key || item.name) === game.id);
    if (!everywhere && !local) {
      notifyError($t('error.game_not_found'));
      return false;
    }
    const result = everywhere
      ? await commands.permanentlyDeleteCloudGame(game.id, true)
      : await commands.deleteGame(local!);
    if (result.status === 'error') {
      notifyError(result.error);
      return false;
    }
    await refreshConfig();
    notifySuccess($t('manage.delete_success'));
    return true;
  }

  return { entries, remove };
}
