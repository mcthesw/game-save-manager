<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { Connection, Download, Refresh, Upload } from '@element-plus/icons-vue';

import {
  commands,
  type CloudArchiveGameView,
  type CloudArchiveLibraryView,
  type CloudArchiveSnapshotView,
  type InitialCatchUpPolicy,
  type SyncMode,
} from '../bindings';
import { $t } from '../i18n';
import { notifyError, notifyInfo, notifySuccess } from '../composables/useActivityCenter';

const feedback = useFeedback();
const library = ref<CloudArchiveLibraryView | null>(null);
const loading = ref(false);
const materializing = ref(false);
const changingMode = ref(false);
const activeTransfer = ref('');
const openGames = ref<string[]>([]);
const modeGame = ref<CloudArchiveGameView | null>(null);
const progressGame = ref<CloudArchiveGameView | null>(null);
const catchUpPolicy = ref<InitialCatchUpPolicy>('keep_remote');

const totalSnapshots = computed(
  () => library.value?.games.reduce((total, game) => total + game.snapshots.length, 0) ?? 0
);
const localSnapshots = computed(
  () => library.value?.games.reduce((total, game) => total + game.local_count, 0) ?? 0
);

function formatBytes(bytes: number | null | undefined) {
  if (!bytes) return $t('sync_settings.archives.size_unknown');
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** index).toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}

function transferKey(gameId: string, snapshotId: string) {
  return `${gameId}\0${snapshotId}`;
}

function catchUpPreview(game: CloudArchiveGameView | null) {
  const snapshots =
    game?.snapshots.filter((snapshot) => snapshot.cloud_verified && !snapshot.local_verified) ?? [];
  return {
    count: snapshots.length,
    size: snapshots.reduce((total, snapshot) => total + (snapshot.size ?? 0), 0),
  };
}

function availabilityLabel(snapshot: CloudArchiveSnapshotView) {
  if (snapshot.local_verified && snapshot.cloud_verified) {
    return $t('sync_settings.archives.available_both');
  }
  if (snapshot.local_verified) return $t('sync_settings.archives.available_local');
  if (snapshot.cloud_verified) return $t('sync_settings.archives.available_cloud');
  if (snapshot.reported_on_devices.length > 0) {
    return $t('sync_settings.archives.available_other_device');
  }
  return $t('sync_settings.archives.unavailable');
}

function availabilityType(snapshot: CloudArchiveSnapshotView) {
  if (snapshot.local_verified && snapshot.cloud_verified) return 'success';
  if (snapshot.local_verified || snapshot.cloud_verified) return 'primary';
  if (snapshot.reported_on_devices.length > 0) return 'warning';
  return 'info';
}

async function load() {
  loading.value = true;
  try {
    const result = await commands.getCloudArchiveLibrary();
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.load_failed'), result.error);
      return;
    }
    library.value = result.data;
    if (openGames.value.length === 0 && result.data.games[0]) {
      openGames.value = [result.data.games[0].game_id];
    }
  } finally {
    loading.value = false;
  }
}

async function transfer(gameId: string, snapshot: CloudArchiveSnapshotView, upload: boolean) {
  const key = transferKey(gameId, snapshot.snapshot_id);
  activeTransfer.value = key;
  try {
    const result = upload
      ? await commands.uploadCloudArchive(gameId, snapshot.snapshot_id)
      : await commands.downloadCloudArchive(gameId, snapshot.snapshot_id);
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.transfer_failed'), result.error);
      return;
    }
    notifySuccess(
      upload
        ? $t('sync_settings.archives.upload_success')
        : $t('sync_settings.archives.download_success')
    );
    await load();
  } finally {
    activeTransfer.value = '';
  }
}

async function materializeAll() {
  const preview = await commands.previewMaterializeAll();
  if (preview.status === 'error') {
    notifyError($t('sync_settings.archives.preview_failed'), preview.error);
    return;
  }
  if (preview.data.snapshot_count === 0) {
    notifyInfo($t('sync_settings.archives.all_local'));
    return;
  }
  try {
    await feedback.confirm(
      $t('sync_settings.archives.download_all_confirm', {
        count: preview.data.snapshot_count,
        size: formatBytes(preview.data.total_bytes),
      }),
      $t('sync_settings.archives.download_all'),
      {
        confirmButtonText: $t('sync_settings.archives.download_all'),
        cancelButtonText: $t('sync_settings.cancel'),
        type: 'info',
      }
    );
  } catch {
    return;
  }
  materializing.value = true;
  try {
    const result = await commands.materializeAllCloudArchives();
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.download_all_failed'), result.error);
      return;
    }
    notifySuccess(
      $t('sync_settings.archives.download_all_success', { count: result.data.downloaded })
    );
    await load();
  } finally {
    materializing.value = false;
  }
}

async function changeMode(game: CloudArchiveGameView, mode: SyncMode) {
  if (mode === game.sync_mode) return;
  if (mode === 'snapshot_sync') {
    modeGame.value = game;
    catchUpPolicy.value = 'keep_remote';
    return;
  }
  await saveMode(game, mode, 'keep_remote');
}

async function saveMode(game: CloudArchiveGameView, mode: SyncMode, policy: InitialCatchUpPolicy) {
  changingMode.value = true;
  try {
    const result = await commands.setGameSyncMode(game.game_id, mode, policy);
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.mode_change_failed'), result.error);
      modeGame.value = null;
      await load();
      return;
    }
    modeGame.value = null;
    notifySuccess(
      result.data.downloaded > 0
        ? $t('sync_settings.archives.mode_enabled_downloaded', {
            count: result.data.downloaded,
          })
        : $t('sync_settings.archives.mode_changed')
    );
    await load();
  } finally {
    changingMode.value = false;
  }
}

onMounted(load);
</script>

<template>
  <section v-loading="loading" class="archive-panel">
    <div class="archive-toolbar">
      <div>
        <h3>{{ $t('sync_settings.archives.title') }}</h3>
        <p>
          {{
            $t('sync_settings.archives.summary', {
              local: localSnapshots,
              total: totalSnapshots,
            })
          }}
        </p>
      </div>
      <div class="toolbar-actions">
        <ElButton :icon="Refresh" circle :aria-label="$t('common.refresh')" @click="load" />
        <ElButton type="primary" :icon="Download" :loading="materializing" @click="materializeAll">
          {{
            library?.pending_materialization
              ? $t('sync_settings.archives.resume_download')
              : $t('sync_settings.archives.download_all')
          }}
        </ElButton>
      </div>
    </div>

    <ElAlert
      v-if="library?.pending_materialization"
      type="info"
      :closable="false"
      show-icon
      :title="$t('sync_settings.archives.resume_hint')"
      class="resume-alert"
    />

    <ElEmpty
      v-if="library && library.games.length === 0"
      :description="$t('sync_settings.overview.no_games')"
    />
    <ElCollapse v-else v-model="openGames">
      <ElCollapseItem v-for="game in library?.games ?? []" :key="game.game_id" :name="game.game_id">
        <template #title>
          <div class="game-summary">
            <div class="game-label">
              <strong>{{ game.name }}</strong>
              <span>
                {{
                  $t('sync_settings.archives.game_summary', {
                    local: game.local_count,
                    cloud: game.cloud_count,
                    total: game.snapshots.length,
                  })
                }}
              </span>
            </div>
            <div class="game-actions">
              <ElButton
                v-if="game.advertised_head_count > 0"
                :icon="Connection"
                type="primary"
                plain
                size="small"
                @click.stop="progressGame = game"
              >
                {{ $t('sync_settings.archives.progress.action') }}
                <ElBadge :value="game.advertised_head_count" />
              </ElButton>
              <ElSelect
                :model-value="game.sync_mode"
                class="sync-mode-select"
                :aria-label="$t('sync_settings.archives.mode')"
                @click.stop
                @change="changeMode(game, $event)"
              >
                <ElOption value="manual" :label="$t('sync_settings.archives.mode_manual')" />
                <ElOption
                  value="snapshot_sync"
                  :label="$t('sync_settings.archives.mode_snapshot')"
                />
              </ElSelect>
            </div>
          </div>
        </template>
        <div v-for="snapshot in game.snapshots" :key="snapshot.snapshot_id" class="snapshot-row">
          <div class="snapshot-info">
            <strong>{{ snapshot.description || snapshot.snapshot_id }}</strong>
            <span>{{ snapshot.snapshot_id }} · {{ formatBytes(snapshot.size) }}</span>
          </div>
          <ElTag :type="availabilityType(snapshot)" effect="plain" round>
            {{ availabilityLabel(snapshot) }}
          </ElTag>
          <ElButton
            v-if="snapshot.local_verified && !snapshot.cloud_verified"
            :icon="Upload"
            text
            :loading="activeTransfer === transferKey(game.game_id, snapshot.snapshot_id)"
            @click="transfer(game.game_id, snapshot, true)"
          >
            {{ $t('sync_settings.archives.upload') }}
          </ElButton>
          <ElButton
            v-else-if="snapshot.cloud_verified && !snapshot.local_verified"
            :icon="Download"
            text
            :loading="activeTransfer === transferKey(game.game_id, snapshot.snapshot_id)"
            @click="transfer(game.game_id, snapshot, false)"
          >
            {{ $t('sync_settings.archives.download') }}
          </ElButton>
          <span v-else class="action-placeholder" />
        </div>
      </ElCollapseItem>
    </ElCollapse>

    <V2ConflictReviewDialog
      v-if="progressGame"
      :model-value="progressGame !== null"
      :game-id="progressGame.game_id"
      :game-name="progressGame.name"
      @update:model-value="progressGame = null"
    />

    <ElDialog
      :model-value="modeGame !== null"
      :title="$t('sync_settings.archives.enable_snapshot_sync')"
      width="min(520px, 92vw)"
      :close-on-click-modal="!changingMode"
      :show-close="!changingMode"
      @close="modeGame = null"
    >
      <ElAlert
        type="warning"
        show-icon
        :closable="false"
        :title="$t('sync_settings.archives.snapshot_sync_risk')"
      />
      <p class="mode-description">
        {{ $t('sync_settings.archives.snapshot_sync_description') }}
      </p>
      <ElRadioGroup v-model="catchUpPolicy" class="catch-up-options">
        <ElRadio value="keep_remote" border>
          <span class="option-copy">
            <strong>{{ $t('sync_settings.archives.keep_remote') }}</strong>
            <small>{{ $t('sync_settings.archives.keep_remote_description') }}</small>
          </span>
        </ElRadio>
        <ElRadio value="download_existing" border>
          <span class="option-copy">
            <strong>{{ $t('sync_settings.archives.download_existing') }}</strong>
            <small>
              {{
                $t('sync_settings.archives.download_existing_description', {
                  count: catchUpPreview(modeGame).count,
                  size: formatBytes(catchUpPreview(modeGame).size),
                })
              }}
            </small>
          </span>
        </ElRadio>
      </ElRadioGroup>
      <template #footer>
        <ElButton :disabled="changingMode" @click="modeGame = null">
          {{ $t('sync_settings.cancel') }}
        </ElButton>
        <ElButton
          type="primary"
          :loading="changingMode"
          @click="modeGame && saveMode(modeGame, 'snapshot_sync', catchUpPolicy)"
        >
          {{ $t('sync_settings.archives.enable') }}
        </ElButton>
      </template>
    </ElDialog>
  </section>
</template>

<style scoped>
.archive-panel {
  min-height: 160px;
}

.archive-toolbar,
.game-summary,
.snapshot-row {
  display: flex;
  align-items: center;
}

.archive-toolbar {
  justify-content: space-between;
  gap: 20px;
  margin-bottom: 16px;
}

.archive-toolbar h3 {
  margin: 0 0 4px;
}

.archive-toolbar p,
.game-summary span,
.snapshot-info span {
  margin: 0;
  color: var(--el-text-color-secondary);
  font-size: 0.82rem;
}

.toolbar-actions {
  display: flex;
  gap: 8px;
  flex-shrink: 0;
}

.game-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.game-actions :deep(.el-badge__content) {
  position: static;
  margin-left: 5px;
  transform: none;
}

.resume-alert {
  margin-bottom: 14px;
}

.game-summary {
  justify-content: space-between;
  gap: 18px;
  width: calc(100% - 24px);
  min-width: 0;
}

.game-label {
  display: grid;
  gap: 2px;
  min-width: 0;
}

.sync-mode-select {
  width: 148px;
  flex-shrink: 0;
}

.snapshot-row {
  min-height: 54px;
  gap: 12px;
  padding: 6px 10px;
  border-bottom: 1px solid var(--el-border-color-lighter);
}

.snapshot-info {
  display: grid;
  flex: 1;
  gap: 3px;
  min-width: 0;
}

.snapshot-info strong,
.snapshot-info span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.snapshot-row :deep(.el-tag) {
  flex-shrink: 0;
}

.snapshot-row :deep(.el-button),
.action-placeholder {
  width: 94px;
  flex-shrink: 0;
}

.mode-description {
  color: var(--el-text-color-regular);
  line-height: 1.6;
}

.catch-up-options {
  display: grid;
  gap: 10px;
}

.catch-up-options :deep(.el-radio) {
  width: 100%;
  height: auto;
  min-height: 68px;
  margin: 0;
  padding: 12px 16px;
}

.catch-up-options :deep(.el-radio__label) {
  min-width: 0;
  white-space: normal;
}

.option-copy {
  display: grid;
  gap: 4px;
}

.option-copy small {
  color: var(--el-text-color-secondary);
  line-height: 1.4;
}

@media (max-width: 640px) {
  .archive-toolbar {
    align-items: flex-start;
    flex-direction: column;
  }

  .snapshot-row {
    align-items: flex-start;
    flex-wrap: wrap;
  }

  .snapshot-info {
    flex-basis: 100%;
  }

  .game-summary {
    align-items: flex-start;
    gap: 10px;
  }

  .game-actions {
    align-items: flex-end;
    flex-direction: column;
  }

  .sync-mode-select {
    width: 132px;
  }
}

@media (max-width: 520px) {
  .game-summary {
    flex-direction: column;
  }

  .sync-mode-select {
    width: 100%;
  }

  .game-actions {
    align-items: stretch;
    width: 100%;
  }
}
</style>
