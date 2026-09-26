import { computed, ref } from 'vue';
import { commands } from '../api/commands';
import type { CheckAppUpdateResponses } from '../api/generated/types.gen';

const update = ref<CheckAppUpdateResponses[200] | null>(null);
const checking = ref(false);
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

async function checkForUpdates(manual = true) {
  if (checking.value) return;
  checking.value = true;
  status.value = 'idle';
  try {
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

export function useAppUpdates() {
  return {
    update,
    checking,
    status,
    showBanner: computed(() =>
      Boolean(update.value?.available && update.value.latestVersion !== dismissedVersion.value)
    ),
    checkForUpdates,
    openRelease: () => openUrl(update.value?.releaseUrl),
    manualDownload: () => openUrl(update.value?.downloadUrl),
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
