<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { commands, type CloudLibraryCutoverReview } from '~/bindings';
import { $t } from '~/i18n';
import { LAYER } from '~/ui/layers';

const props = defineProps<{
  modelValue: boolean;
  resumable?: boolean;
}>();
const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'cutover', gameCount: number): void;
}>();

const review = ref<CloudLibraryCutoverReview | null>(null);
const loading = ref(false);
const running = ref(false);
const visible = computed({
  get: () => props.modelValue,
  set: (value) => emit('update:modelValue', value),
});

async function loadReview() {
  loading.value = true;
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
  if (!review.value || running.value) return;
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
    :title="
      props.resumable
        ? $t('sync_settings.library.cutover.resume_title')
        : $t('sync_settings.library.cutover.title')
    "
    width="min(560px, 94vw)"
    class="cutover-dialog"
    destroy-on-close
    :close-on-click-modal="!running"
    :close-on-press-escape="!running"
    :show-close="!running"
    :z-index="LAYER.dialog"
  >
    <div v-loading="loading || running" class="cutover-body">
      <template v-if="review">
        <p class="cutover-story">
          {{
            props.resumable
              ? $t('sync_settings.library.cutover.resume_story')
              : $t('sync_settings.library.cutover.story')
          }}
        </p>
        <template v-if="!props.resumable">
          <p class="cutover-after-title">{{ $t('sync_settings.library.cutover.after_title') }}</p>
          <ul class="cutover-after">
            <li>{{ $t('sync_settings.library.cutover.after_this_device') }}</li>
            <li>{{ $t('sync_settings.library.cutover.after_old_clients') }}</li>
            <li>{{ $t('sync_settings.library.cutover.after_damage') }}</li>
            <li>{{ $t('sync_settings.library.cutover.after_resume') }}</li>
          </ul>
        </template>
        <p v-if="running" class="running-note">
          {{ $t('sync_settings.library.cutover.running') }}
        </p>
      </template>
    </div>

    <template #footer>
      <ElButton :disabled="running" @click="visible = false">
        {{ $t('sync_settings.cancel') }}
      </ElButton>
      <ElButton type="primary" :loading="running" :disabled="!review" @click="submit">
        {{
          props.resumable
            ? $t('sync_settings.library.cutover.resume_start')
            : $t('sync_settings.library.cutover.start')
        }}
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
  min-height: 120px;
}

.cutover-story,
.cutover-after-title,
.running-note {
  margin: 0;
  line-height: 1.55;
}

.cutover-story {
  color: var(--el-text-color-primary);
}

.cutover-after-title {
  margin-top: 18px;
  color: var(--el-text-color-primary);
  font-weight: 600;
}

.cutover-after {
  margin: 8px 0 0;
  padding-left: 1.2em;
  color: var(--el-text-color-regular);
  line-height: 1.55;
}

.cutover-after li + li {
  margin-top: 6px;
}

.running-note {
  margin-top: 16px;
  color: var(--el-text-color-secondary);
}
</style>
