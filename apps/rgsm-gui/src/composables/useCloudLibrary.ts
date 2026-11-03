import { onMounted, onUnmounted, ref, shallowRef, watch } from 'vue';
import { isEqual } from 'lodash-unified';
import {
  commands,
  type CloudArchiveLibraryView,
  type CloudDeviceProfileView,
  type CloudFailure,
  type CloudLibrarySnapshot,
  type DeletedCloudGameView,
} from '../api/commands';
import { events } from '../api/events';
import { useConfig } from './useConfig';
import { $t } from '../i18n';
import { shareUnchangedItems } from '../utils/stableCollections';

const library = shallowRef<CloudArchiveLibraryView | null>(null);
const deletedGames = shallowRef<DeletedCloudGameView[]>([]);
const deviceProfiles = shallowRef<CloudDeviceProfileView[]>([]);
const lastError = ref<string | null>(null);
/** Why the latest round could not read the cloud; cleared by the next round that succeeds. */
const cloudFailure = shallowRef<CloudFailure | null>(null);
/** Unix milliseconds of the last successful cloud check. */
const lastCheckedAt = ref<number | null>(null);
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
  deletedGames.value = [];
  deviceProfiles.value = [];
  lastError.value = null;
  cloudFailure.value = null;
  inFlight = null;
  lastCheckedAt.value = null;
  connectionKey = savedConnectionKey();
}

function applySnapshot(snapshot: CloudLibrarySnapshot) {
  const next = {
    ...snapshot.library,
    games: shareUnchangedItems(
      library.value?.games ?? [],
      snapshot.library.games,
      (game) => game.game_id
    ),
  };
  if (!isEqual(library.value, next)) library.value = next;
  if (!isEqual(deletedGames.value, snapshot.deleted_games)) {
    deletedGames.value = snapshot.deleted_games;
  }
  if (!isEqual(deviceProfiles.value, snapshot.device_profiles)) {
    deviceProfiles.value = snapshot.device_profiles;
  }
}

/**
 * Show what the last reconciliation round found. Opening a page, focusing the
 * window or navigating never contacts the cloud provider: only a round does
 * (a new local snapshot, an explicit refresh, the startup check, or the
 * background timer).
 */
export async function loadCachedCloudLibrary(): Promise<void> {
  if (savedConnectionKey() !== connectionKey) clearCloudLibrary();
  const requestGeneration = generation;
  const result = await commands.getCachedCloudLibrary();
  if (result.status === 'error' || requestGeneration !== generation) return;
  lastCheckedAt.value = result.data.checked_at ?? null;
  cloudFailure.value = result.data.failure ?? null;
  if (result.data.snapshot) applySnapshot(result.data.snapshot);
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
        await loadCachedCloudLibrary();
        if (!library.value) library.value = result.data;
        lastError.value = configLoaded ? null : $t('error.config_load_failed');
      }
    } catch (cause) {
      if (isCurrent()) {
        await loadCachedCloudLibrary();
        lastError.value = cloudFailure.value?.message ?? String(cause);
      }
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

/**
 * Automatic reads must not keep asking a provider that is throttling requests:
 * every extra request extends the block. Explicit user actions skip this check.
 */
export async function cloudReadsPaused(): Promise<boolean> {
  const result = await commands.getCloudRateLimit();
  if (result.status === 'error' || result.data.retry_after_secs === null) return false;
  lastError.value = result.data.message ?? $t('sync_settings.overview.status_unavailable');
  return true;
}

export function useCloudLibrary() {
  return {
    library,
    deletedGames,
    deviceProfiles,
    lastCheckedAt,
    lastError,
    cloudFailure,
    refresh: refreshCloudLibrary,
  };
}

/** One application-owned lifecycle that mirrors the background worker's results. */
export function useCloudLibraryRefresh() {
  let stopped = false;
  const subscription = events.cloudMetadataChanged.listen(async ({ payload: succeeded }) => {
    await useConfig().whenConfigReady();
    const expectedGeneration = generation;
    const isCurrent = () => !stopped && generation === expectedGeneration;
    await useConfig().refreshLibraryConfig(isCurrent);
    if (!isCurrent()) return;
    if (succeeded) {
      lastError.value = null;
      await loadCachedCloudLibrary();
    } else {
      await loadCachedCloudLibrary();
      lastError.value =
        cloudFailure.value?.message ?? $t('sync_settings.overview.status_unavailable');
    }
    if (!isCurrent() || !library.value) return;
    const statuses = useConfig().deviceGameStatuses.value;
    library.value = {
      ...library.value,
      games: library.value.games.map((game) => {
        const status = statuses.find((item) => item.game_id === game.game_id);
        const local = useConfig().config.value.games.find(
          (item) => item.storage_key === game.game_id
        );
        return status
          ? {
              ...game,
              name: local?.name ?? game.name,
              metadata_sync_pending: status.metadata_sync_pending,
              definition_conflict: status.definition_conflict,
            }
          : game;
      }),
    };
  });
  const showCached = () => {
    void loadCachedCloudLibrary();
  };
  const onVisible = () => {
    if (document.visibilityState === 'visible') showCached();
  };
  watch(
    savedConnectionKey,
    () => {
      clearCloudLibrary();
      showCached();
    },
    { flush: 'sync' }
  );
  onMounted(() => {
    window.addEventListener('focus', showCached);
    document.addEventListener('visibilitychange', onVisible);
    showCached();
  });
  onUnmounted(() => {
    stopped = true;
    void subscription.then((stop) => stop());
    window.removeEventListener('focus', showCached);
    document.removeEventListener('visibilitychange', onVisible);
  });
}
