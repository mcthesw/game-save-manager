<script setup lang="ts">
import { computed, ref } from 'vue';
import { useRouter } from 'vue-router';
import {
  commands,
  type CloudArchiveGameView,
  type CloudArchiveLibraryView,
  type SyncMode,
} from '../bindings';
import { notifyError, notifySuccess } from '../composables/useActivityCenter';
import { getGameManagementPath } from '../composables/useGameManagementRoute';
import { $t } from '../i18n';

const router = useRouter();
const feedback = useFeedback();
const library = ref<CloudArchiveLibraryView | null>(null);
const search = ref('');
const modeGame = ref<CloudArchiveGameView | null>(null);
const pendingMode = ref<SyncMode>('snapshot_sync');
const progressGame = ref<CloudArchiveGameView | null>(null);
const busyGameId = ref('');
const fleet = ref<{ load: () => Promise<void> } | null>(null);

const games = computed(() => {
  const keyword = search.value.trim().toLowerCase();
  const rows = library.value?.games ?? [];
  if (!keyword) return rows;
  return rows.filter((game) => game.name.toLowerCase().includes(keyword));
});

const modeOptions = computed(() => [
  { value: 'manual', label: $t('sync_settings.overview.mode_manual') },
  { value: 'snapshot_sync', label: $t('sync_settings.overview.mode_snapshot') },
  { value: 'live_save_sync', label: $t('sync_settings.overview.mode_live') },
]);

function needsProgressChoice(game: CloudArchiveGameView) {
  return game.managed && game.advertised_head_count > 1;
}

function tableRow(row: unknown): CloudArchiveGameView {
  return row as CloudArchiveGameView;
}

function syncStatus(game: CloudArchiveGameView) {
  if (!game.managed) return 'disabled';
  if (game.advertised_head_count > 1) return 'conflict';
  return 'synced';
}

function statusType(status: ReturnType<typeof syncStatus>) {
  if (status === 'synced') return 'success';
  if (status === 'conflict') return 'warning';
  return 'info';
}

function statusLabel(status: ReturnType<typeof syncStatus>) {
  return $t(`sync_settings.overview.status_${status}`);
}

async function reload() {
  await fleet.value?.load();
}

function onLibraryLoaded(next: CloudArchiveLibraryView) {
  library.value = next;
}

async function changeMode(game: CloudArchiveGameView, mode: string | number | boolean) {
  if (!game.managed) return;
  const next = String(mode) as SyncMode;
  if (next === game.sync_mode) return;
  if (next === 'snapshot_sync' || next === 'live_save_sync') {
    modeGame.value = game;
    pendingMode.value = next;
    return;
  }
  const result = await commands.setGameSyncMode(game.game_id, next, 'keep_remote', null);
  if (result.status === 'error') {
    notifyError($t('sync_settings.archives.mode_change_failed'), result.error);
    await reload();
    return;
  }
  notifySuccess($t('sync_settings.archives.mode_changed'));
  await reload();
}

async function setManaged(game: CloudArchiveGameView, managed: boolean) {
  if (!managed) {
    try {
      await feedback.confirm(
        $t('sync_settings.archives.device.stop_confirm'),
        $t('sync_settings.archives.device.stop_title'),
        {
          confirmButtonText: $t('sync_settings.overview.release'),
          cancelButtonText: $t('sync_settings.cancel'),
          type: 'warning',
        }
      );
    } catch {
      return;
    }
  }
  busyGameId.value = game.game_id;
  try {
    const result = await commands.setDeviceGameManaged(game.game_id, managed, !managed);
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.device.management_failed'), result.error);
      return;
    }
    notifySuccess(
      managed
        ? $t('sync_settings.archives.device.manage_success')
        : $t('sync_settings.archives.device.stop_success')
    );
    await reload();
  } finally {
    busyGameId.value = '';
  }
}

function openGame(game: CloudArchiveGameView) {
  void router.push(getGameManagementPath(game.name));
}
</script>

<template>
  <section class="sync-overview">
    <CloudArchivePanel ref="fleet" @loaded="onLibraryLoaded" />

    <ElInput
      v-model="search"
      clearable
      :placeholder="$t('sync_settings.overview.search')"
      class="overview-search"
    />

    <ElTable
      :data="games"
      class="game-table"
      :empty-text="
        library && library.games.length === 0
          ? $t('sync_settings.overview.no_games')
          : $t('sync_settings.overview.no_matches')
      "
    >
      <ElTableColumn :label="$t('sync_settings.overview.game_name')" min-width="220">
        <template #default="{ row }">
          <div class="game-copy">
            <button type="button" class="game-name" @click="openGame(tableRow(row))">
              {{ tableRow(row).name }}
            </button>
            <button
              v-if="needsProgressChoice(tableRow(row))"
              type="button"
              class="game-note"
              @click="progressGame = tableRow(row)"
            >
              {{ $t('sync_settings.overview.progress_needed') }}
            </button>
          </div>
        </template>
      </ElTableColumn>
      <ElTableColumn :label="$t('sync_settings.overview.status')" width="110" align="center">
        <template #default="{ row }">
          <ElTag :type="statusType(syncStatus(tableRow(row)))" size="small" effect="plain" round>
            {{ statusLabel(syncStatus(tableRow(row))) }}
          </ElTag>
        </template>
      </ElTableColumn>
      <ElTableColumn :label="$t('sync_settings.overview.mode')" width="360">
        <template #default="{ row }">
          <ElSegmented
            :model-value="tableRow(row).sync_mode"
            :options="modeOptions"
            :disabled="!tableRow(row).managed"
            size="small"
            class="mode-switch"
            @change="changeMode(tableRow(row), $event)"
          />
        </template>
      </ElTableColumn>
      <ElTableColumn :label="$t('sync_settings.overview.local_sync')" width="104" align="center">
        <template #default="{ row }">
          <ElSwitch
            :model-value="tableRow(row).managed"
            :loading="busyGameId === tableRow(row).game_id"
            @change="setManaged(tableRow(row), Boolean($event))"
          />
        </template>
      </ElTableColumn>
    </ElTable>

    <V2ConflictReviewDialog
      v-if="progressGame"
      :model-value="progressGame !== null"
      :game-id="progressGame.game_id"
      :game-name="progressGame.name"
      @update:model-value="progressGame = null"
      @resolved="
        progressGame = null;
        reload();
      "
    />
    <CloudSyncModeDialog v-model:game="modeGame" :mode="pendingMode" @updated="reload" />
  </section>
</template>

<style scoped>
.overview-search {
  width: 300px;
  margin-bottom: 12px;
}

.game-table {
  width: 100%;

  :deep(.el-table__inner-wrapper::before) {
    display: none;
  }

  :deep(.el-table__header th) {
    background: transparent;
    font-weight: 600;
    color: var(--el-text-color-regular);
  }

  :deep(.el-table__cell) {
    padding: 14px 0;
  }
}

.game-copy {
  min-width: 0;
}

.game-name,
.game-note {
  display: block;
  overflow: hidden;
  max-width: 100%;
  padding: 0;
  border: 0;
  background: none;
  font: inherit;
  text-align: left;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: pointer;
}

.game-name {
  color: var(--el-text-color-primary);
  font-weight: 500;
}

.game-name:hover,
.game-note:hover {
  color: var(--el-text-color-regular);
}

.game-note {
  margin-top: 4px;
  color: var(--el-text-color-secondary);
  font-size: 0.8rem;
}

.mode-switch {
  --el-border-radius-base: 8px;
}
</style>
