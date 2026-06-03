<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import dayjs from 'dayjs';
import { Delete, FolderOpened, Refresh, VideoPlay } from '@element-plus/icons-vue';
import { error as logError } from '@tauri-apps/plugin-log';

import { $t } from '../i18n';
import { commands, type ExtraBackupItem, type Game } from '../bindings';
import { useApplyConfirmation } from '../composables/useApplyConfirmation';
import { useFeedback } from '../composables/useFeedback';
import { useGlobalLoading } from '../composables/useGlobalLoading';

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

const countText = computed(() => {
  if (loading.value) return '';
  return items.value.length > 0 ? String(items.value.length) : '';
});

watch(
  () => props.modelValue,
  (open) => {
    if (open) refresh();
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
  <el-drawer
    :model-value="modelValue"
    :title="$t('manage.extra_backups')"
    size="580px"
    @update:model-value="(v) => emit('update:modelValue', v)"
  >
    <div class="drawer-body">
      <!-- Toolbar -->
      <div class="toolbar">
        <div class="hint">{{ $t('manage.extra_backups_hint') }}</div>
        <div class="toolbar-actions">
          <el-button text :icon="Refresh" :loading="loading" @click="refresh">
            {{ $t('common.refresh') }}
          </el-button>
          <el-button text :icon="FolderOpened" @click="openFolder">
            {{ $t('manage.open_extra_backup_folder') }}
          </el-button>
        </div>
      </div>

      <!-- Empty State -->
      <el-empty
        v-if="!loading && items.length === 0"
        :description="$t('manage.no_extra_backups')"
      />

      <!-- Backup List -->
      <div v-else v-loading="loading" class="backup-list">
        <div v-for="item in items" :key="item.date" class="backup-item">
          <div class="backup-main">
            <div class="backup-time">{{ formatTime(item) }}</div>
            <div class="backup-size">{{ formatFileSize(item.size) }}</div>
          </div>
          <div class="backup-actions">
            <el-button text type="primary" :icon="VideoPlay" @click="restore(item.date)">
              {{ $t('manage.apply') }}
            </el-button>
            <el-button text type="danger" :icon="Delete" @click="del(item.date)">
              {{ $t('manage.delete') }}
            </el-button>
          </div>
        </div>
      </div>

      <!-- Footer Stats -->
      <div v-if="!loading && countText" class="footer">
        <span class="count-text">{{ $t('manage.extra_backups') }}: {{ countText }}</span>
      </div>
    </div>
  </el-drawer>
</template>

<style scoped>
.drawer-body {
  display: flex;
  flex-direction: column;
  gap: 16px;
  height: 100%;
}

/* Toolbar */
.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--el-border-color-lighter);
}

.hint {
  color: var(--el-text-color-secondary);
  font-size: 13px;
  line-height: 1.5;
  flex: 1;
}

.toolbar-actions {
  display: flex;
  gap: 4px;
  flex-shrink: 0;
}

/* Backup List */
.backup-list {
  display: flex;
  flex-direction: column;
  gap: 1px;
  background: var(--el-border-color-lighter);
  border-radius: 8px;
  overflow-y: auto;
  flex: 1;
  min-height: 0;
}

.backup-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 14px 16px;
  background: var(--el-bg-color);
  transition: background-color 0.2s;
}

.backup-item:hover {
  background: var(--el-fill-color-light);
}

.backup-main {
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex: 1;
  min-width: 0;
}

.backup-time {
  font-family: ui-monospace, 'Cascadia Code', 'Consolas', monospace;
  font-size: 14px;
  font-weight: 500;
  color: var(--el-text-color-primary);
}

.backup-size {
  font-size: 13px;
  color: var(--el-text-color-secondary);
}

.backup-actions {
  display: flex;
  gap: 8px;
  flex-shrink: 0;
}

/* Footer */
.footer {
  display: flex;
  justify-content: flex-end;
  padding-top: 8px;
  border-top: 1px solid var(--el-border-color-lighter);
}

.count-text {
  font-size: 13px;
  color: var(--el-text-color-secondary);
}
</style>
