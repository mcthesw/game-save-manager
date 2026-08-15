<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import dayjs from 'dayjs';
import { FolderOpen, Inbox, LoaderCircle, Play, RefreshCw, Trash2 } from '@lucide/vue';
import { error as logError } from '../utils/logger';

import { $t } from '../i18n';
import { commands, type ExtraBackupItem, type Game } from '../api/commands';
import { useApplyConfirmation } from '../composables/useApplyConfirmation';
import { useFeedback } from '../composables/useFeedback';
import { useGlobalLoading } from '../composables/useGlobalLoading';
import { KButton, KDrawer } from '../ui/kit';

const props = defineProps<{
  game: Game;
  modelValue: boolean;
}>();

const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
}>();

const feedback = useFeedback();
const { confirmAndRun } = useApplyConfirmation();
const { withLoading } = useGlobalLoading();

const loading = ref(false);
const items = ref<ExtraBackupItem[]>([]);

const open = computed({
  get: () => props.modelValue,
  set: (value: boolean) => emit('update:modelValue', value),
});

const countText = computed(() => {
  if (loading.value) return '';
  return items.value.length > 0 ? String(items.value.length) : '';
});

watch(
  () => props.modelValue,
  (visible) => {
    if (visible) refresh();
  }
);

watch(
  () => props.game?.name,
  () => {
    if (props.modelValue) refresh();
  }
);

function formatTime(item: ExtraBackupItem): string {
  if (item.modified_time_ms) {
    return dayjs(item.modified_time_ms).format('YYYY-MM-DD HH:mm:ss');
  }
  const raw = item.date.startsWith('Overwrite_') ? item.date.slice('Overwrite_'.length) : item.date;
  const parsed = dayjs(raw, 'YYYY-MM-DD_HH-mm-ss');
  return parsed.isValid() ? parsed.format('YYYY-MM-DD HH:mm:ss') : item.date;
}

function formatFileSize(bytes: number): string {
  if (!bytes) return '-';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

async function refresh() {
  if (!props.game?.name) return;
  loading.value = true;
  try {
    const result = await commands.getGameExtraBackups(props.game);
    if (result.status === 'error') {
      notifyError(result.error);
      items.value = [];
      return;
    }
    items.value = result.data;
  } catch (e) {
    logError(`Failed to refresh extra backups: ${e}`);
    notifyError($t('settings.failed'));
    items.value = [];
  } finally {
    loading.value = false;
  }
}

async function openFolder() {
  try {
    const result = await commands.openExtraBackupFolder(props.game);
    if (result.status === 'error' || !result.data) {
      notifyError($t('error.open_backup_folder_failed'));
    }
  } catch (e) {
    logError(`Failed to open extra backup folder: ${e}`);
    notifyError($t('error.open_backup_folder_failed'));
  }
}

async function restore(date: string) {
  await confirmAndRun('snapshot', async () => {
    await withLoading(
      async () => {
        const result = await commands.restoreExtraBackup(props.game, date);
        if (result.status === 'error') {
          notifyError($t('manage.recover_failed'));
          return;
        }
        notifySuccess($t('manage.recover_success'));
      },
      $t('manage.restoring_backup'),
      $t('manage.wait_for_prompt_hint')
    );
  });
}

async function del(date: string) {
  try {
    await feedback.confirm($t('manage.confirm_delete_prompt'), $t('home.hint'), {
      confirmButtonText: $t('manage.confirm'),
      cancelButtonText: $t('manage.cancel'),
      type: 'warning',
    });
  } catch {
    return;
  }

  const result = await commands.deleteExtraBackup(props.game, date);
  if (result.status === 'error') {
    notifyError($t('error.delete_snapshot_failed'));
    return;
  }
  notifySuccess($t('manage.delete_success'));
  refresh();
}
</script>

<template>
  <KDrawer v-model:open="open" :title="$t('manage.extra_backups')" :width="580">
    <div class="flex h-full flex-col gap-4">
      <!-- Toolbar -->
      <div class="flex items-center justify-between gap-4 border-b border-border pb-3">
        <p class="flex-1 text-xs leading-relaxed text-text-dim">
          {{ $t('manage.extra_backups_hint') }}
        </p>
        <div class="flex shrink-0 gap-1">
          <KButton variant="ghost" size="sm" :loading="loading" @click="refresh">
            <template #icon><RefreshCw :size="13" aria-hidden="true" /></template>
            {{ $t('common.refresh') }}
          </KButton>
          <KButton variant="ghost" size="sm" @click="openFolder">
            <template #icon><FolderOpen :size="13" aria-hidden="true" /></template>
            {{ $t('manage.open_extra_backup_folder') }}
          </KButton>
        </div>
      </div>

      <!-- Loading -->
      <div v-if="loading" class="flex flex-1 items-center justify-center text-text-dim">
        <LoaderCircle :size="22" class="animate-spin" aria-hidden="true" />
      </div>

      <!-- Empty State -->
      <div
        v-else-if="items.length === 0"
        class="flex flex-1 flex-col items-center justify-center gap-2 text-text-dim"
      >
        <Inbox :size="28" aria-hidden="true" />
        <p class="text-sm">{{ $t('manage.no_extra_backups') }}</p>
      </div>

      <!-- Backup List -->
      <div
        v-else
        class="flex min-h-0 flex-1 flex-col overflow-y-auto rounded-md border border-border"
      >
        <div
          v-for="item in items"
          :key="item.date"
          class="flex items-center justify-between gap-4 border-b border-border px-3.5 py-3 last:border-b-0"
        >
          <div class="min-w-0">
            <div class="font-mono text-xs text-text">{{ formatTime(item) }}</div>
            <div class="mt-0.5 font-mono text-[11px] text-text-dim">
              {{ formatFileSize(item.size) }}
            </div>
          </div>
          <div class="flex shrink-0 gap-1">
            <KButton variant="ghost" size="sm" class="text-success" @click="restore(item.date)">
              <template #icon><Play :size="13" aria-hidden="true" /></template>
              {{ $t('manage.apply') }}
            </KButton>
            <KButton variant="ghost" size="sm" class="text-danger" @click="del(item.date)">
              <template #icon><Trash2 :size="13" aria-hidden="true" /></template>
              {{ $t('manage.delete') }}
            </KButton>
          </div>
        </div>
      </div>

      <!-- Footer Stats -->
      <div v-if="!loading && countText" class="shrink-0 text-xs text-text-dim">
        {{ $t('manage.extra_backups') }}: {{ countText }}
      </div>
    </div>
  </KDrawer>
</template>
