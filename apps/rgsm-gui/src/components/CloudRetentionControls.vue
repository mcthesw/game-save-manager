<script setup lang="ts">
import { ref, watch } from 'vue';

import { commands, type CloudArchiveGameView } from '../api/commands';
import { notifyError, notifySuccess } from '../composables/useActivityCenter';
import { $t } from '../i18n';

const props = defineProps<{ game: CloudArchiveGameView }>();
const emit = defineEmits<{ updated: [] }>();
const feedback = useFeedback();
const enabled = ref(props.game.retention_limit !== null);
const limit = ref(props.game.retention_limit ?? 10);
const saving = ref(false);

watch(
  () => props.game.retention_limit,
  (value) => {
    enabled.value = value !== null;
    limit.value = value ?? 10;
  }
);

async function save() {
  const next = enabled.value ? Math.max(1, limit.value) : null;
  const risky =
    next !== null && (props.game.retention_limit == null || next < props.game.retention_limit);
  if (risky) {
    try {
      await feedback.confirm(
        $t('sync_settings.archives.retention.confirm', { count: next }),
        $t('sync_settings.archives.retention.confirm_title'),
        {
          confirmButtonText: $t('sync_settings.archives.retention.enable'),
          cancelButtonText: $t('sync_settings.cancel'),
          type: 'warning',
        }
      );
    } catch {
      enabled.value = props.game.retention_limit !== null;
      limit.value = props.game.retention_limit ?? 10;
      return;
    }
  }
  saving.value = true;
  try {
    const result = await commands.setSharedSnapshotRetention(props.game.game_id, next, risky);
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.retention.save_failed'), result.error);
      return;
    }
    notifySuccess(
      result.data.deleted > 0
        ? $t('sync_settings.archives.retention.saved_with_deletions', {
            count: result.data.deleted,
          })
        : $t('sync_settings.archives.retention.saved')
    );
    emit('updated');
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <div class="retention-controls">
    <div class="retention-copy">
      <strong>{{ $t('sync_settings.archives.retention.title') }}</strong>
      <small>{{ $t('sync_settings.archives.retention.description') }}</small>
    </div>
    <ElSwitch v-model="enabled" :aria-label="$t('sync_settings.archives.retention.title')" />
    <ElInputNumber
      v-if="enabled"
      v-model="limit"
      :min="1"
      :max="1000"
      controls-position="right"
      :aria-label="$t('sync_settings.archives.retention.limit')"
    />
    <ElButton type="primary" plain :loading="saving" @click="save">
      {{ $t('sync_settings.archives.retention.save') }}
    </ElButton>
  </div>
</template>

<style scoped>
.retention-controls {
  display: flex;
  align-items: center;
  gap: 12px;
}

.retention-copy {
  display: grid;
  flex: 1;
  gap: 3px;
}

.retention-copy small {
  color: var(--el-text-color-secondary);
  line-height: 1.35;
}

.retention-controls :deep(.el-input-number) {
  width: 112px;
}

@media (max-width: 640px) {
  .retention-controls {
    align-items: flex-start;
    flex-wrap: wrap;
  }

  .retention-copy {
    flex-basis: 100%;
  }
}
</style>
