<script setup lang="ts">
import { computed } from 'vue';
import { $t } from '../i18n';
import type { ConflictResolution, GameSyncState } from '../api/commands';

const props = defineProps<{
  modelValue: boolean;
  gameName: string;
  state?: GameSyncState | null;
  currentDeviceId?: string;
  resolving?: boolean;
}>();

const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'resolve', resolution: ConflictResolution): void;
}>();

const visible = computed({
  get: () => props.modelValue,
  set: (value: boolean) => emit('update:modelValue', value),
});

function snapshotLabel(value?: string | null): string {
  return value || $t('sync_settings.conflict.no_snapshot');
}

function formatTime(value?: string | null): string {
  if (!value) return $t('sync_settings.conflict.not_synced');
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

function decideLater() {
  visible.value = false;
}
</script>

<template>
  <ElDialog
    v-model="visible"
    :title="$t('sync_settings.conflict.title')"
    width="520px"
    destroy-on-close
  >
    <p class="conflict-summary">
      {{ $t('sync_settings.conflict.summary', { game: gameName }) }}
    </p>

    <div class="progress-grid">
      <section class="progress-panel">
        <h4>{{ $t('sync_settings.conflict.local_title') }}</h4>
        <dl>
          <div>
            <dt>{{ $t('sync_settings.conflict.snapshot') }}</dt>
            <dd>{{ snapshotLabel(state?.last_known_local_head) }}</dd>
          </div>
          <div>
            <dt>{{ $t('sync_settings.conflict.device') }}</dt>
            <dd>{{ currentDeviceId || $t('sync_settings.conflict.current_device') }}</dd>
          </div>
          <div>
            <dt>{{ $t('sync_settings.conflict.last_checked') }}</dt>
            <dd>{{ formatTime(state?.last_sync_at) }}</dd>
          </div>
        </dl>
      </section>

      <section class="progress-panel">
        <h4>{{ $t('sync_settings.conflict.cloud_title') }}</h4>
        <dl>
          <div>
            <dt>{{ $t('sync_settings.conflict.snapshot') }}</dt>
            <dd>{{ snapshotLabel(state?.last_known_remote_head) }}</dd>
          </div>
          <div>
            <dt>{{ $t('sync_settings.conflict.device') }}</dt>
            <dd>{{ $t('sync_settings.conflict.cloud_device') }}</dd>
          </div>
          <div>
            <dt>{{ $t('sync_settings.conflict.last_checked') }}</dt>
            <dd>{{ formatTime(state?.last_sync_at) }}</dd>
          </div>
        </dl>
      </section>
    </div>

    <template #footer>
      <div class="dialog-actions">
        <ElButton :disabled="resolving" @click="decideLater">
          {{ $t('sync_settings.conflict.decide_later') }}
        </ElButton>
        <ElButton
          type="primary"
          plain
          :loading="resolving"
          @click="emit('resolve', 'accept_remote')"
        >
          {{ $t('sync_settings.conflict.accept_remote') }}
        </ElButton>
        <ElButton type="warning" :loading="resolving" @click="emit('resolve', 'keep_local')">
          {{ $t('sync_settings.conflict.keep_local') }}
        </ElButton>
      </div>
    </template>
  </ElDialog>
</template>

<style scoped>
.conflict-summary {
  margin: 0 0 16px;
  color: var(--el-text-color-regular);
  line-height: 1.5;
}

.progress-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.progress-panel {
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
  padding: 12px;
  background: var(--el-fill-color-lighter);
}

.progress-panel h4 {
  margin: 0 0 10px;
  font-size: 0.95rem;
  color: var(--el-text-color-primary);
}

.progress-panel dl {
  display: grid;
  gap: 8px;
  margin: 0;
}

.progress-panel div {
  display: grid;
  gap: 2px;
}

.progress-panel dt {
  color: var(--el-text-color-secondary);
  font-size: 0.78rem;
}

.progress-panel dd {
  margin: 0;
  color: var(--el-text-color-primary);
  font-size: 0.85rem;
  word-break: break-word;
}

.dialog-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

@media (max-width: 560px) {
  .progress-grid {
    grid-template-columns: 1fr;
  }

  .dialog-actions {
    flex-wrap: wrap;
  }
}
</style>
