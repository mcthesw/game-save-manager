import { onMounted, onUnmounted, ref, shallowRef, watch } from 'vue';
import { commands, type CloudArchiveLibraryView } from '../api/commands';
import { useConfig } from './useConfig';
import { $t } from '../i18n';

const library = shallowRef<CloudArchiveLibraryView | null>(null);
const lastError = ref<string | null>(null);
let inFlight: Promise<CloudArchiveLibraryView | null> | null = null;
let generation = 0;
let connectionKey = '';

function savedConnectionKey() {
  const settings = useConfig().config.value.settings.cloud_settings;
  return JSON.stringify([settings?.backend, settings?.root_path]);
}

/** A connection change must not display metadata from the previous library. */
export function clearCloudLibrary() {
  generation += 1;
  library.value = null;
  lastError.value = null;
  inFlight = null;
  connectionKey = savedConnectionKey();
}

/** Share reads; a completed mutation must not reuse a read started before it. */
export async function refreshCloudLibrary(
  afterMutation = false
): Promise<CloudArchiveLibraryView | null> {
  if (savedConnectionKey() !== connectionKey) clearCloudLibrary();
  if (inFlight) {
    if (!afterMutation) return inFlight;
    await inFlight;
    return refreshCloudLibrary();
  }
  if (
    (useConfig().config.value.settings.cloud_settings?.backend?.type ?? 'Disabled') === 'Disabled'
  ) {
    clearCloudLibrary();
    return null;
  }
  const requestGeneration = generation;
  const request = (async () => {
    try {
      const namespace = await commands.getCloudNamespaceGeneration();
      if (namespace.status === 'error') throw new Error(namespace.error);
      if (namespace.data !== 'v2') return null;
      const result = await commands.refreshCloudArchiveLibrary();
      if (result.status === 'error') throw new Error(result.error);
      const configLoaded = await useConfig().refreshLibraryConfig();
      if (requestGeneration === generation) {
        library.value = result.data;
        lastError.value = configLoaded ? null : $t('error.config_load_failed');
      }
    } catch (cause) {
      if (requestGeneration === generation) lastError.value = String(cause);
    }
    return library.value;
  })().finally(() => {
    if (inFlight === request) {
      inFlight = null;
    }
  });
  inFlight = request;
  return request;
}

export function useCloudLibrary() {
  return { library, lastError, refresh: refreshCloudLibrary };
}

/** One application-owned refresh lifecycle, independent of the current page. */
export function useCloudLibraryRefresh() {
  let timer: ReturnType<typeof setInterval> | undefined;
  const refresh = () => {
    void refreshCloudLibrary();
  };
  const onVisible = () => {
    if (document.visibilityState === 'visible') refresh();
  };
  watch(
    savedConnectionKey,
    () => {
      clearCloudLibrary();
      refresh();
    },
    { flush: 'sync' }
  );
  watch(
    () => useConfig().config.value.settings.cloud_settings?.auto_sync_interval,
    (minutes) => {
      if (timer !== undefined) clearInterval(timer);
      timer = setInterval(refresh, Math.max(1, minutes || 5) * 60_000);
    },
    { immediate: true }
  );
  onMounted(() => {
    window.addEventListener('focus', refresh);
    document.addEventListener('visibilitychange', onVisible);
    refresh();
  });
  onUnmounted(() => {
    if (timer !== undefined) clearInterval(timer);
    window.removeEventListener('focus', refresh);
    document.removeEventListener('visibilitychange', onVisible);
  });
}
