import { ref, watch, type Ref } from 'vue';
import { commands, type Game } from '../api/commands';
import { notifyError } from './useActivityCenter';
import { $t } from '../i18n';

/** Keep the requested operation until configuration is saved; closing cancels it. */
export function useVariableSetup(game: Ref<Game>, drawer: Ref<boolean>) {
  const missing = ref<string[]>([]);
  const continuing = ref(false);
  let pending: (() => Promise<void>) | undefined;
  let generation = 0;
  async function refresh() {
    const request = ++generation;
    const result = await commands.missingGameVariables(game.value);
    if (request === generation && result.status === 'ok') missing.value = result.data;
    return result;
  }
  watch(
    game,
    () => {
      pending = undefined;
      continuing.value = false;
      void refresh();
    },
    { deep: true }
  );
  watch(drawer, (open) => {
    if (!open) {
      pending = undefined;
      continuing.value = false;
    }
  });
  async function ensure(resume: () => Promise<void>) {
    try {
      const result = await refresh();
      if (result.status === 'error') {
        notifyError($t('path_variables.setup_unavailable'));
        return false;
      }
      if (!result.data.length) return true;
      pending = resume;
      continuing.value = true;
      drawer.value = true;
      return false;
    } catch {
      notifyError($t('path_variables.setup_unavailable'));
      return false;
    }
  }
  function takeContinuation() {
    const action = pending;
    pending = undefined;
    continuing.value = false;
    return action;
  }
  return { missing, continuing, refresh, ensure, takeContinuation };
}
