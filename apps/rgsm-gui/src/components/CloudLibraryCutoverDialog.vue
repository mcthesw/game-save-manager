<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { commands, type CloudLibraryCutoverReview } from '~/bindings';
import { $t } from '~/i18n';
import { LAYER } from '~/ui/layers';
const props = defineProps<{ modelValue: boolean }>();
const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'cutover', gameCount: number): void;
}>();
const review = ref<CloudLibraryCutoverReview | null>(null);
const acknowledged = ref(false);
const loading = ref(false);
const running = ref(false);
const visible = computed({
  get: () => props.modelValue,
  set: (value) => emit('update:modelValue', value),
});
function formatBytes(bytes: number) {
  if (bytes <= 0) return $t('sync_settings.library.cutover.size_unknown');
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** index).toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}
async function loadReview() {
  loading.value = true;
  acknowledged.value = false;
  try {
    const result = await commands.reviewCloudLibraryCutover();
    if (result.status === 'error') {
      notifyError(`${$t('sync_settings.library.cutover.review_failed')}: ${result.error}`);
      visible.value = false;
      return;
    }
    review.value = result.data;
  } catch (reason) {
    notifyError(`${$t('sync_settings.library.cutover.review_failed')}: ${String(reason)}`);
    visible.value = false;
  } finally {
    loading.value = false;
  }
}
async function submit() {
  if (!review.value || !acknowledged.value || running.value) return;
  running.value = true;
  try {
    const result = await commands.cutoverCloudLibrary(true);
    if (result.status === 'error') {
      notifyError(`${$t('sync_settings.library.cutover.failed')}: ${result.error}`);
      return;
    }
    const outcome = result.data;
    if (outcome.unavailable_archives > 0) {
      notifyWarning(
        $t('sync_settings.library.cutover.completed_with_warnings', {
          count: outcome.unavailable_archives,
        })
      );
    } else {
      notifySuccess($t('sync_settings.library.cutover.completed'));
    }
    emit('cutover', outcome.game_count);
    visible.value = false;
  } catch (reason) {
    notifyError(`${$t('sync_settings.library.cutover.failed')}: ${String(reason)}`);
  } finally {
    running.value = false;
  }
}
watch(
  () => props.modelValue,
  (open) => {
    if (open) void loadReview();
  }
);
</script>
<template>
  <ElDialog
    v-model="visible"
    :title="$t('sync_settings.library.cutover.title')"
    width="min(720px, 94vw)"
    class="cutover-dialog"
    destroy-on-close
    :close-on-click-modal="!running"
    :close-on-press-escape="!running"
    :show-close="!running"
    :z-index="LAYER.dialog"
  >
    <div v-loading="loading || running" class="cutover-body">
      <template v-if="review">
        <div class="cutover-summary">
          <div>
            <strong>{{ review.game_count }}</strong>
            <span>{{ $t('sync_settings.library.cutover.games') }}</span>
          </div>
          <div>
            <strong>{{ review.snapshot_count }}</strong>
            <span>{{ $t('sync_settings.library.cutover.snapshots') }}</span>
          </div>
          <div>
            <strong>{{ formatBytes(review.declared_bytes) }}</strong>
            <span>{{ $t('sync_settings.library.cutover.declared_size') }}</span>
          </div>
        </div>

        <ElAlert
          type="warning"
          :title="$t('sync_settings.library.cutover.compatibility_title')"
          :description="$t('sync_settings.library.cutover.compatibility_description')"
          :closable="false"
          show-icon
        />
        <ElAlert
          type="info"
          :title="$t('sync_settings.library.cutover.archive_title')"
          :description="$t('sync_settings.library.cutover.archive_description')"
          :closable="false"
          show-icon
        />
        <ElCheckbox v-model="acknowledged" :disabled="running" class="acknowledgement">
          {{ $t('sync_settings.library.cutover.acknowledgement') }}
        </ElCheckbox>
        <p v-if="running" class="running-note">
          {{ $t('sync_settings.library.cutover.running') }}
        </p>
      </template>
    </div>

    <template #footer>
      <ElButton :disabled="running" @click="visible = false">
        {{ $t('sync_settings.cancel') }}
      </ElButton>
      <ElButton
        type="primary"
        :loading="running"
        :disabled="!review || !acknowledged"
        @click="submit"
      >
        {{ $t('sync_settings.library.cutover.start') }}
      </ElButton>
    </template>
  </ElDialog>
</template>
<style>
.cutover-dialog {
  max-height: calc(100vh - 32px);
  margin: 16px auto;
  display: flex;
  flex-direction: column;
}
.cutover-dialog .el-dialog__body {
  min-height: 0;
  overflow-y: auto;
}
</style>
<style scoped>
.cutover-body {
  min-height: 220px;
}
.cutover-summary {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
  margin-bottom: 16px;
}
.cutover-summary div {
  min-width: 0;
  padding: 14px;
  border-radius: var(--el-border-radius-base);
  background: var(--el-fill-color-light);
}
.cutover-summary strong,
.cutover-summary span {
  display: block;
  overflow-wrap: anywhere;
}
.cutover-summary strong {
  font-size: 20px;
  color: var(--el-text-color-primary);
}
.cutover-summary span,
.running-note {
  margin-top: 4px;
  color: var(--el-text-color-secondary);
}
.cutover-body .el-alert + .el-alert {
  margin-top: 12px;
}
.acknowledgement {
  height: auto;
  margin-top: 18px;
  white-space: normal;
}

@media (max-width: 560px) {
  .cutover-summary {
    grid-template-columns: 1fr;
  }

  .cutover-dialog :deep(.el-dialog__footer) {
    padding-right: calc(var(--el-dialog-padding-primary) + var(--el-component-size-large) + 12px);
  }
}
</style>
