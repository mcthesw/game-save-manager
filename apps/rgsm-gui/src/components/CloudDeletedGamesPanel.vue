<script setup lang="ts">
import { Refresh } from '@element-plus/icons-vue';

import { commands, type DeletedCloudGameView } from '../bindings';
import { $t } from '../i18n';
import { notifyError, notifySuccess } from '../composables/useActivityCenter';

const emit = defineEmits<{
  updated: [];
}>();

const games = ref<DeletedCloudGameView[]>([]);
const loading = ref(false);
const retrying = ref('');

async function load() {
  loading.value = true;
  try {
    const result = await commands.getDeletedCloudGames();
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.games.load_deleted_failed'), result.error);
      return;
    }
    games.value = result.data;
  } finally {
    loading.value = false;
  }
}

async function retry(game: DeletedCloudGameView) {
  retrying.value = game.game_id;
  try {
    const result = await commands.permanentlyDeleteCloudGame(game.game_id, true);
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.games.delete_incomplete'), result.error);
    } else {
      notifySuccess(
        $t('sync_settings.archives.games.delete_success', {
          snapshots: result.data.removed_snapshots,
        })
      );
    }
    await load();
    emit('updated');
  } finally {
    retrying.value = '';
  }
}

onMounted(load);
</script>

<template>
  <section v-if="loading || games.length > 0" v-loading="loading" class="deleted-games">
    <div>
      <strong>{{ $t('sync_settings.archives.games.deleted_title') }}</strong>
      <p>{{ $t('sync_settings.archives.games.deleted_description') }}</p>
    </div>
    <div v-for="game in games" :key="game.game_id" class="deleted-row">
      <span>
        <strong>{{ game.name }}</strong>
        <small>{{ game.game_id }}</small>
      </span>
      <div>
        <ElTag :type="game.deletion_incomplete ? 'warning' : 'info'" effect="plain">
          {{
            game.deletion_incomplete
              ? $t('sync_settings.archives.games.incomplete')
              : $t('sync_settings.archives.games.removed')
          }}
        </ElTag>
        <ElButton
          v-if="game.deletion_incomplete"
          :icon="Refresh"
          text
          type="warning"
          :loading="retrying === game.game_id"
          @click="retry(game)"
        >
          {{ $t('sync_settings.archives.games.retry') }}
        </ElButton>
      </div>
    </div>
  </section>
</template>

<style scoped>
.deleted-games {
  display: grid;
  gap: 8px;
}

.deleted-games p {
  margin: 4px 0 0;
  color: var(--el-text-color-secondary);
  font-size: 0.82rem;
}

.deleted-row,
.deleted-row > span,
.deleted-row > div {
  display: flex;
}

.deleted-row {
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding-top: 8px;
  border-top: 1px solid var(--el-border-color-lighter);
}

.deleted-row > span {
  min-width: 0;
  flex-direction: column;
}

.deleted-row small {
  overflow: hidden;
  color: var(--el-text-color-secondary);
  text-overflow: ellipsis;
}

.deleted-row > div {
  align-items: center;
  gap: 6px;
}
</style>
