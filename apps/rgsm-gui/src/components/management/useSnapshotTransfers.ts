import { computed, type Ref } from 'vue';
import { commands, type CloudArchiveGameView, type Game, type Snapshot } from '../../api/commands';
import { $t } from '../../i18n';
import {
  canDownloadSnapshot,
  canEvictSnapshot,
  canUploadSnapshot,
  isRetentionProtectedDate,
} from './snapshotAvailability';

/**
 * Cloud archive row operations for the management page: single-snapshot
 * transfers (up/download/evict/remove), pending-deletion retry, retention
 * protection, and the selection-driven batch variants. Local lifecycle
 * (create/apply/delete local snapshots) stays in the page.
 */
export function useSnapshotTransfers(deps: {
  game: Ref<Game>;
  cloudGame: Ref<CloudArchiveGameView | null>;
  activeTransfer: Ref<string>;
  retentionProtectedDates: Ref<Set<string>>;
  /** Currently selected rows (drives batch availability). */
  selected: () => Snapshot[];
  /** All rows in the local table (legacy-namespace describe lookup). */
  allSnapshots: () => Snapshot[];
  refresh: () => Promise<void>;
}) {
  const {
    game,
    cloudGame,
    activeTransfer,
    retentionProtectedDates,
    selected,
    allSnapshots,
    refresh,
  } = deps;
  const feedback = useFeedback();

  const selectedUploadable = computed(() =>
    selected().filter((snapshot) => canUploadSnapshot(cloudGame.value, snapshot.date))
  );
  const selectedDownloadable = computed(() =>
    selected().filter((snapshot) => canDownloadSnapshot(cloudGame.value, snapshot.date))
  );
  const selectedEvictable = computed(() =>
    selected().filter((snapshot) => canEvictSnapshot(cloudGame.value, snapshot.date))
  );

  function gameId() {
    return game.value.storage_key || game.value.name;
  }

  async function transferSnapshot(date: string, upload: boolean) {
    activeTransfer.value = date;
    try {
      const result = upload
        ? await commands.uploadCloudArchive(gameId(), date)
        : await commands.downloadCloudArchive(gameId(), date);
      if (result.status === 'error') {
        notifyError($t('sync_settings.archives.transfer_failed'), result.error);
        return;
      }
      notifySuccess(
        upload
          ? $t('sync_settings.archives.upload_success')
          : $t('sync_settings.archives.download_success')
      );
      await refresh();
    } finally {
      activeTransfer.value = '';
    }
  }

  async function retryPendingDeletion(snapshotId: string, retryable: boolean) {
    if (!retryable) return;
    activeTransfer.value = snapshotId;
    try {
      const result = await commands.deleteV2Snapshot(gameId(), snapshotId, false);
      if (result.status === 'error') {
        notifyError($t('sync_settings.archives.delete_incomplete'), result.error);
        return;
      }
      notifySuccess($t('sync_settings.archives.delete_success'));
      await refresh();
    } finally {
      activeTransfer.value = '';
    }
  }

  async function evictSnapshot(date: string) {
    try {
      await feedback.confirm(
        $t('sync_settings.archives.evict.confirm', { snapshot: date }),
        $t('sync_settings.archives.evict.title'),
        {
          confirmButtonText: $t('sync_settings.archives.evict.action'),
          cancelButtonText: $t('sync_settings.cancel'),
          type: 'warning',
        }
      );
    } catch {
      return;
    }
    activeTransfer.value = date;
    try {
      const result = await commands.evictLocalArchive(gameId(), date, true);
      if (result.status === 'error') {
        notifyError($t('sync_settings.archives.evict.failed'), result.error);
        return;
      }
      notifySuccess($t('sync_settings.archives.evict.success'));
      await refresh();
    } finally {
      activeTransfer.value = '';
    }
  }

  async function batchTransfer(upload: boolean) {
    const rows = upload ? selectedUploadable.value : selectedDownloadable.value;
    for (const snapshot of rows) {
      await transferSnapshot(snapshot.date, upload);
    }
  }

  async function batchEvict() {
    const rows = selectedEvictable.value;
    if (rows.length === 0) return;
    try {
      await feedback.confirm(
        $t('manage.batch_evict_confirm', { count: rows.length }),
        $t('sync_settings.archives.evict.title'),
        {
          confirmButtonText: $t('sync_settings.archives.evict.action'),
          cancelButtonText: $t('sync_settings.cancel'),
          type: 'warning',
        }
      );
    } catch {
      return;
    }
    let succeeded = 0;
    for (const snapshot of rows) {
      activeTransfer.value = snapshot.date;
      const result = await commands.evictLocalArchive(gameId(), snapshot.date, true);
      if (result.status === 'error') {
        notifyError($t('sync_settings.archives.evict.failed'), result.error);
        break;
      }
      succeeded += 1;
    }
    activeTransfer.value = '';
    if (succeeded === rows.length) {
      notifySuccess($t('manage.batch_evict_success', { count: succeeded }));
    } else if (succeeded > 0) {
      notifyError(
        $t('manage.batch_evict_partial', {
          succeeded,
          failed: rows.length - succeeded,
        })
      );
    }
    await refresh();
  }

  function isRetentionProtected(date: string) {
    return isRetentionProtectedDate(retentionProtectedDates.value, cloudGame.value, date);
  }

  async function convertToPermanent(snapshotDate: string) {
    try {
      const generation = await commands.getCloudNamespaceGeneration();
      if (generation.status === 'error') {
        notifyError(generation.error);
        return;
      }
      if (generation.data === 'v2') {
        const nextProtected = !isRetentionProtected(snapshotDate);
        if (nextProtected) {
          await feedback.confirm(
            $t('manage.protect_from_retention_confirm'),
            $t('manage.convert_to_permanent'),
            {
              confirmButtonText: $t('manage.convert_to_permanent'),
              cancelButtonText: $t('manage.cancel'),
              type: 'info',
            }
          );
        } else {
          await feedback.confirm(
            $t('sync_settings.archives.retention.unprotect_confirm'),
            $t('sync_settings.archives.retention.unprotect_title'),
            {
              confirmButtonText: $t('sync_settings.archives.retention.unprotect'),
              cancelButtonText: $t('manage.cancel'),
              type: 'warning',
            }
          );
        }
        const result = await commands.setSnapshotRetentionProtected(
          gameId(),
          snapshotDate,
          nextProtected,
          !nextProtected
        );
        if (result.status === 'error') {
          notifyError($t('sync_settings.archives.retention.protection_failed'), result.error);
          return;
        }
        notifySuccess(
          nextProtected
            ? $t('manage.protect_from_retention_success')
            : $t('sync_settings.archives.retention.unprotected')
        );
        await refresh();
        return;
      }
      const snapshot = allSnapshots().find((x) => x.date === snapshotDate);
      const { value } = await feedback.prompt(
        $t('manage.input_description_prompt'),
        $t('manage.convert_to_permanent'),
        {
          confirmButtonText: $t('manage.confirm'),
          cancelButtonText: $t('manage.cancel'),
          inputValue: snapshot?.describe,
        }
      );
      if (value !== snapshot?.describe) {
        const descResult = await commands.setSnapshotDescription(game.value, snapshotDate, value);
        if (descResult.status === 'error') {
          notifyError($t('manage.change_description_failed'));
          return;
        }
      }
      const result = await commands.setSnapshotCreatedBy(game.value.name, snapshotDate, 'Manual');
      if (result.status === 'error') {
        notifyError($t('manage.convert_to_permanent_failed'));
        return;
      }
      notifySuccess($t('manage.convert_to_permanent_success'));
      await refresh();
    } catch {
      notifyInfo($t('manage.operation_canceled'));
    }
  }

  return {
    selectedUploadable,
    selectedDownloadable,
    selectedEvictable,
    transferSnapshot,
    retryPendingDeletion,
    evictSnapshot,
    batchTransfer,
    batchEvict,
    convertToPermanent,
  };
}
