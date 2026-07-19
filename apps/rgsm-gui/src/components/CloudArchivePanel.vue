<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { Download, Refresh, Upload } from '@element-plus/icons-vue';

import { commands, type CloudArchiveLibraryView, type CloudArchiveSnapshotView } from '../bindings';
import { $t } from '../i18n';
import { notifyError, notifyInfo, notifySuccess } from '../composables/useActivityCenter';

const feedback = useFeedback();
const library = ref<CloudArchiveLibraryView | null>(null);
const loading = ref(false);
const materializing = ref(false);
const activeTransfer = ref('');
const openGames = ref<string[]>([]);

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
          {{ $t('sync_settings.archives.download_all') }}
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

.resume-alert {
  margin-bottom: 14px;
}

.game-summary {
  gap: 12px;
  min-width: 0;
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
    flex-direction: column;
    gap: 2px;
  }
}
</style>
