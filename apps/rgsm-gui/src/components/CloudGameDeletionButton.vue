<script setup lang="ts">
import { Delete } from '@element-plus/icons-vue';

import { commands } from '../api/commands';
import { $t } from '../i18n';
import { notifyError, notifySuccess } from '../composables/useActivityCenter';

const props = defineProps<{
  gameId: string;
  gameName: string;
}>();
const emit = defineEmits<{
  deleted: [];
}>();

const feedback = useFeedback();
const deleting = ref(false);

async function permanentlyDelete() {
  try {
    await feedback.confirm(
      $t('sync_settings.archives.games.delete_confirm', { game: props.gameName }),
      $t('sync_settings.archives.games.delete_title'),
      {
        confirmButtonText: $t('sync_settings.archives.games.delete_action'),
        cancelButtonText: $t('sync_settings.cancel'),
        type: 'error',
      }
    );
  } catch {
    return;
  }

  deleting.value = true;
  try {
    const result = await commands.permanentlyDeleteCloudGame(props.gameId, true);
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.games.delete_incomplete'), result.error);
      emit('deleted');
      return;
    }
    notifySuccess(
      $t('sync_settings.archives.games.delete_success', {
        snapshots: result.data.removed_snapshots,
      })
    );
    emit('deleted');
  } finally {
    deleting.value = false;
  }
}
</script>

<template>
  <div class="danger-zone">
    <div>
      <strong>{{ $t('sync_settings.archives.games.danger_title') }}</strong>
      <p>{{ $t('sync_settings.archives.games.danger_description') }}</p>
    </div>
    <ElButton :icon="Delete" type="danger" plain :loading="deleting" @click="permanentlyDelete">
      {{ $t('sync_settings.archives.games.delete_action') }}
    </ElButton>
  </div>
</template>

<style scoped>
.danger-zone {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 18px;
  margin-top: 16px;
}

.danger-zone p {
  margin: 4px 0 0;
  color: var(--el-text-color-secondary);
  font-size: 0.82rem;
  line-height: 1.45;
}

@media (max-width: 640px) {
  .danger-zone {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
