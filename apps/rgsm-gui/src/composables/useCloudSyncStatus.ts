import { computed, ref, shallowRef } from 'vue';

import { commands, events } from '../api/commands';
import { $t } from '../i18n';
import { notifyError, notifyInfo } from './useActivityCenter';

export type CloudSyncJobStatus = 'Queued' | 'Running' | 'Completed' | 'Failed' | 'Cancelled';

export interface CloudSyncJobInfo {
  id: number;
  description: string;
  status: CloudSyncJobStatus;
  error?: string | null;
}

type CloudSyncStatusPayload = {
  active_jobs: number;
  current_description?: string | null;
  jobs: CloudSyncJobInfo[];
};

const activeJobs = ref(0);
const currentDescription = ref('');
const jobs = shallowRef<CloudSyncJobInfo[]>([]);
const isCancelling = ref(false);
const initialized = ref(false);

function applyStatus(payload: CloudSyncStatusPayload) {
  activeJobs.value = Math.max(0, payload.active_jobs ?? 0);
  currentDescription.value = payload.current_description?.trim() || '';
  jobs.value = payload.jobs ?? [];
}

function initListeners() {
  if (typeof window === 'undefined' || initialized.value) {
    return;
  }

  initialized.value = true;

  events.cloudSyncStatusEvent
    .listen((event) => {
      applyStatus(event.payload);
    })
    .catch((err) => {
      notifyError($t('cloud_sync.listen_failed'), String(err));
    });

  events.cloudSyncErrorEvent
    .listen((event) => {
      const payload = event.payload;
      const gameName = payload.game_name?.trim();
      if (gameName) {
        notifyError(
          $t('cloud_sync.failed_with_game', {
            game: gameName,
            error: payload.error,
          })
        );
        return;
      }

      notifyError(
        $t('cloud_sync.failed', {
          error: payload.error,
        })
      );
    })
    .catch((err) => {
      notifyError($t('cloud_sync.listen_failed'), String(err));
    });
}

export function useCloudSyncStatus() {
  initListeners();

  const isSyncing = computed(() => activeJobs.value > 0);
  const displayDescription = computed(
    () => currentDescription.value || $t('cloud_sync.default_description')
  );

  async function cancelSync() {
    if (isCancelling.value) {
      return;
    }

    isCancelling.value = true;
    try {
      const result = await commands.cancelCloudSync();
      if (result.status === 'error') {
        notifyError(result.error);
        return;
      }
      if (result.data === 'cancelled') {
        notifyInfo($t('cloud_sync.cancelled'));
      }
    } finally {
      isCancelling.value = false;
    }
  }

  return {
    activeJobs,
    currentDescription,
    displayDescription,
    isSyncing,
    isCancelling,
    jobs,
    cancelSync,
  };
}
