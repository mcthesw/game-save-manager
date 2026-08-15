<script setup lang="ts">
import { computed } from 'vue';
import {
  ArrowDown,
  ArrowUp,
  CloudCheck,
  Download,
  FolderMinus,
  Inbox,
  Lock,
  LockOpen,
  Pencil,
  Play,
  Trash2,
  Upload,
} from '@lucide/vue';
import type { CloudArchiveGameView, Snapshot } from '../../api/commands';
import { $t } from '../../i18n';
import { KButton, KCheckbox, KTag, KTooltip } from '../../ui/kit';
import {
  canApplySnapshot,
  canDownloadSnapshot,
  canEvictSnapshot,
  canUploadSnapshot,
  isRetentionProtectedDate,
  isSnapshotInCloud,
  isSnapshotOnDevice,
  snapshotLocationLabel,
} from './snapshotAvailability';

const props = defineProps<{
  rows: Snapshot[];
  sortDesc: boolean;
  selectedDates: Set<string>;
  cloudGame: CloudArchiveGameView | null;
  localCatalogDates: Set<string>;
  retentionProtectedDates: Set<string>;
  activeTransfer: string;
}>();

const emit = defineEmits<{
  toggleSort: [];
  toggleSelect: [date: string, checked: boolean];
  toggleSelectAll: [checked: boolean];
  apply: [date: string];
  remove: [date: string];
  changeDescribe: [date: string];
  convertPermanent: [date: string];
  evict: [date: string];
  download: [date: string];
  upload: [date: string];
}>();

const GRID_COLS = 'grid-cols-[2.25rem_11.5rem_minmax(0,1fr)_8rem_11.75rem]';

const selectedCount = computed(
  () => props.rows.filter((snapshot) => props.selectedDates.has(snapshot.date)).length
);
const isAllSelected = computed(
  () => props.rows.length > 0 && selectedCount.value === props.rows.length
);
const isIndeterminate = computed(
  () => props.rows.length > 0 && selectedCount.value > 0 && !isAllSelected.value
);
const headerChecked = computed<boolean | 'indeterminate'>({
  get: () => (isAllSelected.value ? true : isIndeterminate.value ? 'indeterminate' : false),
  set: (value) => emit('toggleSelectAll', value === true),
});

function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}

function snapshotSourceTag(snapshot: Snapshot): string | null {
  if (snapshot.created_by === 'Timer') return $t('manage.snapshot_source_timer');
  if (snapshot.created_by === 'Tray') return $t('manage.snapshot_source_tray');
  if (snapshot.created_by === 'Hotkey') return $t('manage.snapshot_source_hotkey');
  if (snapshot.created_by === 'ProcessStart') return $t('manage.snapshot_source_process_start');
  if (snapshot.created_by === 'ProcessExit') return $t('manage.snapshot_source_process_exit');
  if (snapshot.created_by === 'ProcessInterval')
    return $t('manage.snapshot_source_process_interval');
  return null;
}

function isAutomaticSnapshot(snapshot: Snapshot): boolean {
  return (
    snapshot.created_by === 'Timer' ||
    snapshot.created_by === 'ProcessStart' ||
    snapshot.created_by === 'ProcessExit' ||
    snapshot.created_by === 'ProcessInterval'
  );
}

const isOnDevice = (date: string) => isSnapshotOnDevice(props.cloudGame, date);
const isInCloud = (date: string) => isSnapshotInCloud(props.cloudGame, date);
const canUpload = (date: string) => canUploadSnapshot(props.cloudGame, date);
const canDownload = (date: string) => canDownloadSnapshot(props.cloudGame, date);
const canEvict = (date: string) => canEvictSnapshot(props.cloudGame, date);
const canApply = (date: string) => canApplySnapshot(props.localCatalogDates, props.cloudGame, date);
const isProtected = (date: string) =>
  isRetentionProtectedDate(props.retentionProtectedDates, props.cloudGame, date);
const locationLabel = (date: string) => snapshotLocationLabel(props.cloudGame, date);
</script>

<template>
  <div class="flex h-full min-h-0 flex-col">
    <div
      v-if="rows.length === 0"
      class="flex flex-1 flex-col items-center justify-center gap-2 text-text-dim"
    >
      <Inbox :size="28" aria-hidden="true" />
      <p class="text-sm">{{ $t('manage.no_snapshots') }}</p>
    </div>

    <div v-else class="min-h-0 flex-1 overflow-x-hidden overflow-y-auto">
      <!-- Header -->
      <div
        class="sticky top-0 z-10 grid h-10 items-center gap-2 border-b border-border bg-surface px-3 text-xs font-medium text-text-dim"
        :class="GRID_COLS"
      >
        <div class="flex items-center justify-center">
          <KCheckbox v-model="headerChecked" :aria-label="$t('manage.batch_delete')" />
        </div>
        <button
          type="button"
          class="inline-flex cursor-pointer items-center gap-1 border-none bg-transparent p-0 text-xs font-medium text-text-dim transition-colors hover:text-text"
          @click="emit('toggleSort')"
        >
          {{ $t('manage.save_date') }}
          <ArrowDown v-if="sortDesc" :size="12" aria-hidden="true" />
          <ArrowUp v-else :size="12" aria-hidden="true" />
        </button>
        <span>{{ $t('manage.description') }}</span>
        <span>{{ $t('manage.location_and_size') }}</span>
        <span class="text-center">{{ $t('manage.actions') }}</span>
      </div>

      <!-- Rows -->
      <div
        v-for="snapshot in rows"
        :key="snapshot.date"
        class="grid h-11 items-center gap-2 border-b border-border px-3 transition-colors hover:bg-surface-2/60"
        :class="[GRID_COLS, { 'bg-surface-2/40': selectedDates.has(snapshot.date) }]"
      >
        <div class="flex items-center justify-center">
          <KCheckbox
            :model-value="selectedDates.has(snapshot.date)"
            :aria-label="snapshot.date"
            @update:model-value="emit('toggleSelect', snapshot.date, $event === true)"
          />
        </div>

        <span class="truncate font-mono text-xs text-text">{{ snapshot.date }}</span>

        <div class="flex min-w-0 items-center gap-1.5">
          <KTag v-if="snapshotSourceTag(snapshot)">{{ snapshotSourceTag(snapshot) }}</KTag>
          <span
            v-if="snapshot.describe"
            class="truncate text-sm text-text"
            :title="snapshot.describe"
          >
            {{ snapshot.describe }}
          </span>
          <span v-else class="truncate text-sm text-text-dim/70">{{
            $t('manage.no_description')
          }}</span>
        </div>

        <div class="flex min-w-0 flex-col gap-0.5">
          <span class="truncate text-[11px] leading-tight text-text-dim">{{
            locationLabel(snapshot.date)
          }}</span>
          <span class="font-mono text-xs leading-tight text-text">{{
            snapshot.size ? formatFileSize(snapshot.size) : '-'
          }}</span>
        </div>

        <div class="flex items-center justify-center">
          <span class="inline-flex h-7 w-7 items-center justify-center">
            <KTooltip
              v-if="cloudGame && isOnDevice(snapshot.date)"
              :content="$t('manage.local_remove')"
            >
              <KButton
                variant="ghost"
                size="sm"
                :aria-label="$t('manage.local_remove')"
                :disabled="!canEvict(snapshot.date)"
                :loading="activeTransfer === snapshot.date"
                @click="emit('evict', snapshot.date)"
              >
                <template #icon><FolderMinus :size="15" aria-hidden="true" /></template>
              </KButton>
            </KTooltip>
            <KTooltip
              v-else-if="cloudGame && !isOnDevice(snapshot.date)"
              :content="
                canDownload(snapshot.date)
                  ? $t('manage.local_download')
                  : $t('manage.local_unavailable')
              "
            >
              <KButton
                variant="ghost"
                size="sm"
                :aria-label="$t('manage.local_download')"
                :disabled="!canDownload(snapshot.date)"
                :loading="activeTransfer === snapshot.date"
                @click="emit('download', snapshot.date)"
              >
                <template #icon><Download :size="15" aria-hidden="true" /></template>
              </KButton>
            </KTooltip>
          </span>

          <!-- 云端槽:可上传时是上传按钮;已在云端时是被动状态标(云副本删除走行尾「删除」) -->
          <span class="inline-flex h-7 w-7 items-center justify-center">
            <KTooltip
              v-if="cloudGame && canUpload(snapshot.date)"
              :content="$t('manage.cloud_upload')"
            >
              <KButton
                variant="ghost"
                size="sm"
                :aria-label="$t('manage.cloud_upload')"
                :loading="activeTransfer === snapshot.date"
                @click="emit('upload', snapshot.date)"
              >
                <template #icon><Upload :size="15" aria-hidden="true" /></template>
              </KButton>
            </KTooltip>
            <span
              v-else-if="cloudGame && isInCloud(snapshot.date)"
              class="inline-flex items-center justify-center"
              :title="locationLabel(snapshot.date)"
            >
              <CloudCheck :size="15" class="text-text-dim" aria-hidden="true" />
            </span>
          </span>

          <span class="inline-flex h-7 w-7 items-center justify-center">
            <KTooltip
              :content="
                canApply(snapshot.date) ? $t('manage.apply') : $t('manage.download_before_apply')
              "
            >
              <KButton
                variant="ghost"
                size="sm"
                class="text-success"
                :aria-label="$t('manage.apply')"
                :disabled="!canApply(snapshot.date)"
                @click="emit('apply', snapshot.date)"
              >
                <template #icon><Play :size="15" aria-hidden="true" /></template>
              </KButton>
            </KTooltip>
          </span>

          <span class="inline-flex h-7 w-7 items-center justify-center">
            <KTooltip
              v-if="isAutomaticSnapshot(snapshot) && !isProtected(snapshot.date)"
              :content="$t('manage.convert_to_permanent')"
            >
              <KButton
                variant="ghost"
                size="sm"
                :aria-label="$t('manage.convert_to_permanent')"
                @click="emit('convertPermanent', snapshot.date)"
              >
                <template #icon><Lock :size="15" aria-hidden="true" /></template>
              </KButton>
            </KTooltip>
            <KTooltip
              v-else-if="isAutomaticSnapshot(snapshot) && isProtected(snapshot.date)"
              :content="$t('sync_settings.archives.retention.unprotect')"
            >
              <KButton
                variant="ghost"
                size="sm"
                :aria-label="$t('sync_settings.archives.retention.unprotect')"
                @click="emit('convertPermanent', snapshot.date)"
              >
                <template #icon><LockOpen :size="15" aria-hidden="true" /></template>
              </KButton>
            </KTooltip>
            <KTooltip
              v-else
              :content="
                localCatalogDates.has(snapshot.date)
                  ? $t('manage.change_describe')
                  : $t('manage.download_before_apply')
              "
            >
              <KButton
                variant="ghost"
                size="sm"
                :aria-label="$t('manage.change_describe')"
                :disabled="!localCatalogDates.has(snapshot.date)"
                @click="emit('changeDescribe', snapshot.date)"
              >
                <template #icon><Pencil :size="15" aria-hidden="true" /></template>
              </KButton>
            </KTooltip>
          </span>

          <span class="inline-flex h-7 w-7 items-center justify-center">
            <KTooltip :content="$t('manage.delete')">
              <KButton
                variant="ghost"
                size="sm"
                class="text-danger"
                :aria-label="$t('manage.delete')"
                @click="emit('remove', snapshot.date)"
              >
                <template #icon><Trash2 :size="15" aria-hidden="true" /></template>
              </KButton>
            </KTooltip>
          </span>
        </div>
      </div>
    </div>
  </div>
</template>
