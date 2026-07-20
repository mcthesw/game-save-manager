<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import {
  Connection,
  Delete as DeleteIcon,
  Download,
  Refresh,
  Upload,
} from '@element-plus/icons-vue';

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
import {
  canProtectCloudArchiveSnapshot as canProtect,
  cloudArchiveAvailabilityLabel as availabilityLabel,
  cloudArchiveAvailabilityType as availabilityType,
  cloudArchiveCatchUpPreview as catchUpPreview,
  cloudArchiveTransferKey as transferKey,
  formatCloudArchiveBytes as formatBytes,
} from '../utils/cloudArchivePresentation';

const feedback = useFeedback();
const library = ref<CloudArchiveLibraryView | null>(null);
const loading = ref(false);
const materializing = ref(false);
const changingMode = ref(false);
const activeTransfer = ref('');
const activeDeletion = ref('');
const openGames = ref<string[]>([]);
const modeGame = ref<CloudArchiveGameView | null>(null);
const pendingMode = ref<SyncMode>('snapshot_sync');
const progressGame = ref<CloudArchiveGameView | null>(null);
const catchUpPolicy = ref<InitialCatchUpPolicy>('keep_remote');
const liveSaveProcessName = ref('');
const liveSaveSnapshotOnExit = ref(false);

const totalSnapshots = computed(
  () => library.value?.games.reduce((total, game) => total + game.snapshots.length, 0) ?? 0
);
const localSnapshots = computed(
  () => library.value?.games.reduce((total, game) => total + game.local_count, 0) ?? 0
);

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

async function deleteSnapshot(
  gameId: string,
  snapshotId: string,
  label: string,
  confirmed: boolean
) {
  const key = transferKey(gameId, snapshotId);
  if (confirmed) {
    try {
      await feedback.confirm(
        $t('sync_settings.archives.delete_confirm', { snapshot: label }),
        $t('sync_settings.archives.delete_title'),
        {
          confirmButtonText: $t('sync_settings.archives.delete_permanently'),
          cancelButtonText: $t('sync_settings.cancel'),
          type: 'warning',
        }
      );
    } catch {
      return;
    }
  }
  activeDeletion.value = key;
  try {
    const result = await commands.deleteV2Snapshot(gameId, snapshotId, confirmed);
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.delete_incomplete'), result.error);
    } else {
      notifySuccess($t('sync_settings.archives.delete_success'));
    }
    await load();
  } finally {
    activeDeletion.value = '';
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
  if (mode === 'snapshot_sync' || mode === 'live_save_sync') {
    modeGame.value = game;
    pendingMode.value = mode;
    catchUpPolicy.value = 'keep_remote';
    liveSaveProcessName.value = game.live_save_process_name ?? '';
    liveSaveSnapshotOnExit.value = game.live_save_snapshot_on_exit;
    return;
  }
  await saveMode(game, mode, 'keep_remote', null);
}

async function saveMode(
  game: CloudArchiveGameView,
  mode: SyncMode,
  policy: InitialCatchUpPolicy,
  liveSave: { process_name: string; snapshot_on_exit: boolean } | null
) {
  changingMode.value = true;
  try {
    const result = await commands.setGameSyncMode(game.game_id, mode, policy, liveSave);
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
    <CloudArchiveToolbar
      :local-snapshots="localSnapshots"
      :total-snapshots="totalSnapshots"
      :materializing="materializing"
      :pending-materialization="library?.pending_materialization ?? false"
      @refresh="load"
      @download-all="materializeAll"
    />

    <ElAlert
      v-if="library?.pending_materialization"
      type="info"
      :closable="false"
      show-icon
      :title="$t('sync_settings.archives.resume_hint')"
      class="resume-alert"
    />

    <CloudDeviceProfilesPanel />
    <CloudDeletedGamesPanel @updated="load" />

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
              <CloudDeviceGameControls :game="game" @updated="load" />
              <ElButton
                v-if="game.managed && game.advertised_head_count > 0"
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
                v-if="game.managed"
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
                <ElOption value="live_save_sync" :label="$t('sync_settings.archives.mode_live')" />
              </ElSelect>
            </div>
          </div>
        </template>
        <CloudRetentionControls :game="game" @updated="load" />
        <div v-for="snapshot in game.snapshots" :key="snapshot.snapshot_id" class="snapshot-row">
          <div class="snapshot-info">
            <strong>{{ snapshot.description || snapshot.snapshot_id }}</strong>
            <span>{{ snapshot.snapshot_id }} · {{ formatBytes(snapshot.size) }}</span>
          </div>
          <ElTag :type="availabilityType(snapshot)" effect="plain" round>
            {{ availabilityLabel(snapshot) }}
          </ElTag>
          <div class="snapshot-actions">
            <LocalArchiveEvictionButton
              v-if="snapshot.local_verified"
              :game-id="game.game_id"
              :snapshot-id="snapshot.snapshot_id"
              :label="snapshot.description || snapshot.snapshot_id"
              @updated="load"
            />
            <SnapshotRetentionButton
              v-if="canProtect(snapshot)"
              :game-id="game.game_id"
              :snapshot="snapshot"
              @updated="load"
            />
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
            <ElButton
              :icon="DeleteIcon"
              text
              type="danger"
              :loading="activeDeletion === transferKey(game.game_id, snapshot.snapshot_id)"
              @click="
                deleteSnapshot(
                  game.game_id,
                  snapshot.snapshot_id,
                  snapshot.description || snapshot.snapshot_id,
                  true
                )
              "
            >
              {{ $t('sync_settings.archives.delete') }}
            </ElButton>
          </div>
        </div>
        <div
          v-for="deletion in game.pending_deletions"
          :key="`deleting-${deletion.snapshot_id}`"
          class="snapshot-row pending-deletion"
        >
          <div class="snapshot-info">
            <strong>{{ deletion.description || deletion.snapshot_id }}</strong>
            <span>{{ deletion.snapshot_id }}</span>
          </div>
          <ElTag type="warning" effect="plain" round>
            {{ $t('sync_settings.archives.deletion_pending') }}
          </ElTag>
          <ElButton
            v-if="deletion.retryable"
            :icon="Refresh"
            text
            type="warning"
            :loading="activeDeletion === transferKey(game.game_id, deletion.snapshot_id)"
            @click="
              deleteSnapshot(
                game.game_id,
                deletion.snapshot_id,
                deletion.description || deletion.snapshot_id,
                false
              )
            "
          >
            {{ $t('sync_settings.archives.retry_delete') }}
          </ElButton>
          <span v-else class="deletion-waiting">
            {{ $t('sync_settings.archives.deletion_waiting') }}
          </span>
        </div>
        <CloudGameDeletionButton :game-id="game.game_id" :game-name="game.name" @deleted="load" />
      </ElCollapseItem>
    </ElCollapse>

    <V2ConflictReviewDialog
      v-if="progressGame"
      :model-value="progressGame !== null"
      :game-id="progressGame.game_id"
      :game-name="progressGame.name"
      @update:model-value="progressGame = null"
      @resolved="
        progressGame = null;
        load();
      "
    />

    <ElDialog
      :model-value="modeGame !== null"
      :title="
        pendingMode === 'live_save_sync'
          ? $t('sync_settings.archives.enable_live_save_sync')
          : $t('sync_settings.archives.enable_snapshot_sync')
      "
      width="min(520px, 92vw)"
      :close-on-click-modal="!changingMode"
      :show-close="!changingMode"
      @close="modeGame = null"
    >
      <ElAlert
        type="warning"
        show-icon
        :closable="false"
        :title="
          pendingMode === 'live_save_sync'
            ? $t('sync_settings.archives.live_save_sync_risk')
            : $t('sync_settings.archives.snapshot_sync_risk')
        "
      />
      <p class="mode-description">
        {{
          pendingMode === 'live_save_sync'
            ? $t('sync_settings.archives.live_save_sync_description')
            : $t('sync_settings.archives.snapshot_sync_description')
        }}
      </p>
      <div v-if="pendingMode === 'live_save_sync'" class="live-save-options">
        <label>
          <span>{{ $t('sync_settings.archives.live_save_process') }}</span>
          <ElInput
            v-model="liveSaveProcessName"
            :placeholder="$t('sync_settings.archives.live_save_process_placeholder')"
          />
        </label>
        <div class="snapshot-exit-option">
          <span>
            <strong>{{ $t('sync_settings.archives.snapshot_on_exit') }}</strong>
            <small>{{ $t('sync_settings.archives.snapshot_on_exit_description') }}</small>
          </span>
          <ElSwitch v-model="liveSaveSnapshotOnExit" />
        </div>
      </div>
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
          :disabled="pendingMode === 'live_save_sync' && !liveSaveProcessName.trim()"
          :loading="changingMode"
          @click="
            modeGame &&
            saveMode(
              modeGame,
              pendingMode,
              catchUpPolicy,
              pendingMode === 'live_save_sync'
                ? {
                    process_name: liveSaveProcessName,
                    snapshot_on_exit: liveSaveSnapshotOnExit,
                  }
                : null
            )
          "
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

.game-summary,
.snapshot-row {
  display: flex;
  align-items: center;
}

.game-summary span,
.snapshot-info span {
  margin: 0;
  color: var(--el-text-color-secondary);
  font-size: 0.82rem;
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

.snapshot-actions {
  display: flex;
  justify-content: flex-end;
  min-width: 180px;
  flex-shrink: 0;
}

.pending-deletion {
  background: var(--el-color-warning-light-9);
}

.deletion-waiting {
  max-width: 180px;
  color: var(--el-text-color-secondary);
  font-size: 0.78rem;
  text-align: right;
}

.mode-description {
  color: var(--el-text-color-regular);
  line-height: 1.6;
}

.live-save-options {
  display: grid;
  gap: 14px;
  margin-bottom: 16px;
}

.live-save-options label,
.snapshot-exit-option span {
  display: grid;
  gap: 5px;
}

.snapshot-exit-option {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.snapshot-exit-option small {
  color: var(--el-text-color-secondary);
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
  .snapshot-row {
    align-items: flex-start;
    flex-wrap: wrap;
  }

  .snapshot-info {
    flex-basis: 100%;
  }

  .snapshot-actions {
    flex: 1;
    min-width: 0;
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
