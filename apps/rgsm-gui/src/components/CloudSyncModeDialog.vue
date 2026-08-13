<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import {
  commands,
  type CloudArchiveGameView,
  type InitialCatchUpPolicy,
  type SyncMode,
} from '../bindings';
import { notifyError, notifySuccess } from '../composables/useActivityCenter';
import { $t } from '../i18n';
import {
  cloudArchiveCatchUpPreview as catchUpPreview,
  formatCloudArchiveBytes as formatBytes,
} from '../utils/cloudArchivePresentation';

const props = defineProps<{
  game: CloudArchiveGameView | null;
  mode: SyncMode;
}>();

const emit = defineEmits<{
  (event: 'update:game', value: CloudArchiveGameView | null): void;
  (event: 'updated'): void;
}>();

const changingMode = ref(false);
const catchUpPolicy = ref<InitialCatchUpPolicy>('keep_remote');
const liveSaveProcessName = ref('');
const liveSaveSnapshotOnExit = ref(false);
const isLive = computed(() => props.mode === 'live_save_sync');

const visible = computed({
  get: () => props.game !== null,
  set: (value: boolean) => {
    if (!value) emit('update:game', null);
  },
});

watch(
  () => props.game,
  (game) => {
    catchUpPolicy.value = 'keep_remote';
    liveSaveProcessName.value = game?.live_save_process_name ?? '';
    liveSaveSnapshotOnExit.value = game?.live_save_snapshot_on_exit ?? false;
  }
);

async function confirm() {
  const game = props.game;
  if (!game) return;
  changingMode.value = true;
  try {
    const result = await commands.setGameSyncMode(
      game.game_id,
      props.mode,
      catchUpPolicy.value,
      isLive.value
        ? {
            process_name: liveSaveProcessName.value,
            snapshot_on_exit: liveSaveSnapshotOnExit.value,
          }
        : null
    );
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.mode_change_failed'), result.error);
      emit('update:game', null);
      emit('updated');
      return;
    }
    emit('update:game', null);
    notifySuccess(
      result.data.downloaded > 0
        ? $t('sync_settings.archives.mode_enabled_downloaded', {
            count: result.data.downloaded,
          })
        : $t('sync_settings.archives.mode_changed')
    );
    emit('updated');
  } finally {
    changingMode.value = false;
  }
}
</script>

<template>
  <ElDialog
    v-model="visible"
    :title="
      isLive
        ? $t('sync_settings.archives.enable_live_save_sync')
        : $t('sync_settings.archives.enable_snapshot_sync')
    "
    width="min(480px, 92vw)"
    :close-on-click-modal="!changingMode"
    :show-close="!changingMode"
  >
    <ElAlert
      type="warning"
      show-icon
      :closable="false"
      :title="
        isLive
          ? $t('sync_settings.archives.live_save_sync_risk')
          : $t('sync_settings.archives.snapshot_sync_risk')
      "
    />
    <ul class="mode-points">
      <li>
        {{
          isLive
            ? $t('sync_settings.archives.live_save_sync_description')
            : $t('sync_settings.archives.snapshot_sync_description')
        }}
      </li>
      <li>
        {{
          isLive
            ? $t('sync_settings.archives.live_save_sync_description_2')
            : $t('sync_settings.archives.snapshot_sync_description_2')
        }}
      </li>
      <li v-if="isLive">
        {{ $t('sync_settings.archives.live_save_sync_description_3') }}
      </li>
    </ul>
    <div v-if="isLive" class="live-save-options">
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
    <p class="catch-up-title">{{ $t('sync_settings.archives.catch_up_title') }}</p>
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
                count: catchUpPreview(game).count,
                size: formatBytes(catchUpPreview(game).size),
              })
            }}
          </small>
        </span>
      </ElRadio>
    </ElRadioGroup>
    <template #footer>
      <ElButton :disabled="changingMode" @click="visible = false">
        {{ $t('sync_settings.cancel') }}
      </ElButton>
      <ElButton
        type="primary"
        :disabled="isLive && !liveSaveProcessName.trim()"
        :loading="changingMode"
        @click="confirm"
      >
        {{ $t('sync_settings.archives.enable') }}
      </ElButton>
    </template>
  </ElDialog>
</template>

<style scoped>
.mode-points {
  margin: 12px 0 16px;
  padding-left: 18px;
  color: var(--el-text-color-regular);
  line-height: 1.55;
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

.catch-up-title {
  margin: 0 0 8px;
  color: var(--el-text-color-regular);
  font-weight: 600;
}

.catch-up-options {
  display: grid;
  gap: 10px;
}

.catch-up-options :deep(.el-radio) {
  width: 100%;
  height: auto;
  min-height: 60px;
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
</style>
