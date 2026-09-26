import { computed, ref } from 'vue';
import { commands, events } from '../api/commands';
import type { CheckAppUpdateResponses, UpdateProgress } from '../api/generated/types.gen';

type UpdateCheck = CheckAppUpdateResponses[200];
const update = ref<UpdateCheck | null>(null);
const checking = ref(false);
const pending = ref(false);
const state = ref<UpdateProgress | null>(null);
const status = ref<'idle' | 'current' | 'check-error' | 'install-error'>('idle');
const dismissalKey = 'rgsm.dismissed-app-update';
function readDismissal() {
  try {
    return localStorage.getItem(dismissalKey) ?? '';
  } catch {
    return '';
  }
}
const dismissedVersion = ref(readDismissal());
let initialized: Promise<void> | undefined;

function receive(progress: UpdateProgress) {
  state.value = progress;
  if (progress.update) update.value = progress.update;
  status.value = progress.stage === 'failed' ? 'install-error' : 'idle';
}

function initializeUpdates() {
  if (!initialized)
    initialized = (async () => {
      // Subscribe before reading state, so navigation never loses a running download.
      await events.appUpdateProgress.listen(({ payload }) => receive(payload));
      try {
        receive(await commands.getAppUpdateState());
      } catch (cause) {
        console.warn('Could not read update state', cause);
      }
    })();
  return initialized;
}

const busy = computed(
  () => pending.value || ['downloading', 'waiting', 'installing'].includes(state.value?.stage ?? '')
);
const ready = computed(() => state.value?.stage === 'ready');
const progressPercent = computed(() =>
  state.value?.totalBytes
    ? Math.min(100, Math.round((state.value.downloadedBytes / state.value.totalBytes) * 100))
    : null
);

async function checkForUpdates(manual = true) {
  if (checking.value || busy.value) return;
  checking.value = true;
  status.value = 'idle';
  try {
    await initializeUpdates();
    if (busy.value || ready.value) return;
    update.value = await commands.checkAppUpdate();
    if (manual) dismissedVersion.value = '';
    if (!update.value.available) status.value = 'current';
  } catch (cause) {
    status.value = 'check-error';
    console.warn('Update check failed', cause);
  } finally {
    checking.value = false;
  }
}

async function openUrl(url?: string | null) {
  if (!url) return;
  try {
    const result = await commands.openUrl(url);
    if (result.status === 'error') throw new Error(result.error);
  } catch (cause) {
    status.value = 'install-error';
    console.warn('Could not open update page', cause);
  }
}

async function applyUpdate() {
  const available = update.value;
  if (!available?.available || busy.value) return;
  if (available.action === 'download') return openUrl(available.downloadUrl);
  pending.value = true;
  status.value = 'idle';
  try {
    await initializeUpdates();
    if (ready.value) await commands.installAppUpdate(available.latestVersion);
    else await commands.downloadAppUpdate(available.latestVersion);
    receive(await commands.getAppUpdateState());
  } catch (cause) {
    status.value = 'install-error';
    console.warn('Update operation failed', cause);
  } finally {
    pending.value = false;
  }
}

export function useAppUpdates() {
  return {
    update,
    checking,
    busy,
    ready,
    status,
    state,
    progressPercent,
    showBanner: computed(() =>
      Boolean(update.value?.available && update.value.latestVersion !== dismissedVersion.value)
    ),
    initializeUpdates,
    checkForUpdates,
    applyUpdate,
    openRelease: () => openUrl(update.value?.releaseUrl),
    manualDownload: () => openUrl(update.value?.downloadUrl),
    cancelInstall: async () => {
      try {
        await commands.cancelAppUpdateInstall();
      } catch (cause) {
        console.warn('Could not cancel update wait', cause);
      }
    },
    dismiss: () => {
      dismissedVersion.value = update.value?.latestVersion ?? '';
      try {
        localStorage.setItem(dismissalKey, dismissedVersion.value);
      } catch {
        /* Storage may be unavailable. */
      }
    },
  };
}
