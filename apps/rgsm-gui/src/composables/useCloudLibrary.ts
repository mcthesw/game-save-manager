import { onMounted, onUnmounted, ref, shallowRef, watch } from 'vue';
import { isEqual } from 'lodash-unified';
import { commands, type CloudArchiveLibraryView } from '../api/commands';
import { useConfig } from './useConfig';
import { $t } from '../i18n';
import { shareUnchangedItems } from '../utils/stableCollections';

const library = shallowRef<CloudArchiveLibraryView | null>(null);
const lastError = ref<string | null>(null);
let inFlight: Promise<CloudArchiveLibraryView | null> | null = null;
let generation = 0;
let connectionKey = '';
const lastRefreshFinishedAt = ref<number | null>(null);

function refreshIntervalMs() {
  const minutes = useConfig().config.value.settings.cloud_settings?.auto_sync_interval;
  return Math.max(1, minutes || 5) * 60_000;
}

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
  lastRefreshFinishedAt.value = null;
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
  const isCurrent = () => requestGeneration === generation;
  const request = (async () => {
    try {
      const namespace = await commands.getCloudNamespaceGeneration();
      if (namespace.status === 'error') throw new Error(namespace.error);
      if (namespace.data !== 'v2') return null;
      const result = await commands.refreshCloudArchiveLibrary();
      if (result.status === 'error') throw new Error(result.error);
      if (!isCurrent()) return library.value;
      const configLoaded = await useConfig().refreshLibraryConfig(isCurrent);
      if (isCurrent()) {
        const next = {
          ...result.data,
          games: shareUnchangedItems(
            library.value?.games ?? [],
            result.data.games,
            (game) => game.game_id
          ),
        };
        if (!isEqual(library.value, next)) library.value = next;
        lastError.value = configLoaded ? null : $t('error.config_load_failed');
      }
    } catch (cause) {
      if (isCurrent()) lastError.value = String(cause);
    }
    return library.value;
  })().finally(() => {
    if (isCurrent()) lastRefreshFinishedAt.value = Date.now();
    if (inFlight === request) {
      inFlight = null;
    }
  });
  inFlight = request;
  return request;
}

/** Passive checks share the configured interval, including after failed attempts. */
export function refreshCloudLibraryIfStale(): Promise<CloudArchiveLibraryView | null> {
  if (savedConnectionKey() !== connectionKey) clearCloudLibrary();
  const finishedAt = lastRefreshFinishedAt.value;
  const age = finishedAt === null ? Infinity : Date.now() - finishedAt;
  if (age >= 0 && age < refreshIntervalMs()) {
    return Promise.resolve(library.value);
  }
  return refreshCloudLibrary();
}

export function useCloudLibrary() {
  return { library, lastError, refresh: refreshCloudLibrary };
}

/** One application-owned refresh lifecycle, independent of the current page. */
export function useCloudLibraryRefresh() {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let stopped = false;
  const refresh = () => {
    void refreshCloudLibraryIfStale();
  };
  const schedule = () => {
    if (timer !== undefined) clearTimeout(timer);
    if (stopped) return;
    const interval = refreshIntervalMs();
    const elapsed =
      lastRefreshFinishedAt.value === null ? 0 : Date.now() - lastRefreshFinishedAt.value;
    timer = setTimeout(
      async () => {
        await refreshCloudLibraryIfStale();
        schedule();
      },
      Math.max(0, interval - Math.max(0, elapsed))
    );
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
  watch([refreshIntervalMs, lastRefreshFinishedAt], schedule, { immediate: true });
  onMounted(() => {
    window.addEventListener('focus', refresh);
    document.addEventListener('visibilitychange', onVisible);
    refresh();
  });
  onUnmounted(() => {
    stopped = true;
    if (timer !== undefined) clearTimeout(timer);
    window.removeEventListener('focus', refresh);
    document.removeEventListener('visibilitychange', onVisible);
  });
}
