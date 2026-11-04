import { computed, ref, shallowRef } from 'vue';

import type { TransferProgress } from '../api/generated';
import { commands, events } from '../api/commands';
import { $t } from '../i18n';
import { notifyError, notifyInfo } from './useActivityCenter';

export type CloudSyncJobStatus = 'Queued' | 'Running' | 'Completed' | 'Failed' | 'Cancelled';

export interface CloudSyncJobInfo {
  id: number;
  description: string;
  status: CloudSyncJobStatus;
  error?: string | null;
  progress?: TransferProgress | null;
}

type CloudSyncStatusPayload = {
  active_jobs: number;
  current_description?: string | null;
  jobs: CloudSyncJobInfo[];
};

const activeJobs = ref(0);
const jobs = shallowRef<CloudSyncJobInfo[]>([]);
const isCancelling = ref(false);
const initialized = ref(false);

function applyStatus(payload: CloudSyncStatusPayload) {
  activeJobs.value = Math.max(0, payload.active_jobs ?? 0);
  jobs.value = payload.jobs ?? [];
}

function initListeners() {
  if (typeof window === 'undefined' || initialized.value) {
    return;
  }

  initialized.value = true;

  void events.cloudSyncStatusEvent.listen((event) => {
    applyStatus(event.payload);
  });
}

export function useCloudSyncStatus() {
  initListeners();

  const isSyncing = computed(() => activeJobs.value > 0);

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
    isSyncing,
    isCancelling,
    jobs,
    cancelSync,
  };
}
