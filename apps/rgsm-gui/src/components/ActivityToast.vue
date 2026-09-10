<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue';
import { CheckCircle2, Info, LoaderCircle, TriangleAlert, X, XCircle } from '@lucide/vue';
import { useActivityCenter } from '../composables/useActivityCenter';
import { useCloudSyncStatus } from '../composables/useCloudSyncStatus';
import { LAYER } from '../ui/layers';
import { $t } from '../i18n';

const { activities, activityAddSignal } = useActivityCenter();
const { isSyncing, jobs } = useCloudSyncStatus();
const visible = ref(false);
const source = ref<'activity' | 'cloud'>('activity');
const activityId = ref<string>();
let timer: ReturnType<typeof setTimeout> | undefined;

const feedback = computed(() => {
  if (source.value === 'activity')
    return activities.value.find((entry) => entry.id === activityId.value);
  const job = jobs.value.find((entry) => entry.status === 'Running') ?? jobs.value[0];
  if (!job) return undefined;
  return {
    title: job.description,
    description: job.error ?? undefined,
    status: isSyncing.value
      ? 'running'
      : job.status === 'Failed'
        ? 'error'
        : job.status === 'Cancelled'
          ? 'info'
          : 'success',
  };
});
const icon = computed(() => {
  switch (feedback.value?.status) {
    case 'running':
    case 'pending':
      return LoaderCircle;
    case 'success':
      return CheckCircle2;
    case 'warning':
      return TriangleAlert;
    case 'error':
      return XCircle;
    default:
      return Info;
  }
});
const running = computed(() => ['pending', 'running'].includes(feedback.value?.status ?? ''));

function close() {
  clearTimeout(timer);
  visible.value = false;
}
function pauseClose() {
  clearTimeout(timer);
}
function scheduleClose() {
  clearTimeout(timer);
  if (!feedback.value) {
    visible.value = false;
    return;
  }
  if (!running.value) {
    const delay = ['error', 'warning'].includes(feedback.value.status) ? 20_000 : 4_000;
    timer = setTimeout(close, delay);
  }
}
watch(activityAddSignal, () => {
  source.value = 'activity';
  activityId.value = activities.value.at(-1)?.id;
  visible.value = true;
  scheduleClose();
});
watch(isSyncing, (syncing) => {
  if (syncing) {
    source.value = 'cloud';
    visible.value = true;
  }
});
watch(feedback, scheduleClose, { deep: true });
onUnmounted(() => clearTimeout(timer));
</script>

<template>
  <div
    v-if="visible && feedback"
    class="activity-toast pointer-events-auto fixed left-1/2 top-5 box-border flex w-[min(420px,calc(100vw-40px))] -translate-x-1/2 items-start gap-2 rounded-md border border-border bg-surface px-3 py-2.5 text-sm text-text shadow-overlay"
    :style="{ zIndex: LAYER.toast }"
    role="status"
    aria-live="polite"
    aria-atomic="true"
    @pointerdown.stop
    @mouseenter="pauseClose"
    @mouseleave="scheduleClose"
  >
    <component
      :is="icon"
      :size="16"
      class="mt-0.5 shrink-0"
      :class="{
        'animate-spin': running,
        'text-danger': feedback.status === 'error',
        'text-warning': feedback.status === 'warning',
        'text-success': feedback.status === 'success',
      }"
      aria-hidden="true"
    />
    <div class="min-w-0 flex-1 max-h-[30vh] overflow-y-auto break-words">
      <div>{{ feedback.title }}</div>
      <div v-if="feedback.description" class="mt-1 text-xs text-text-dim">
        {{ feedback.description }}
      </div>
    </div>
    <button
      type="button"
      :aria-label="$t('common.close')"
      class="inline-flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-sm border-none bg-transparent text-text-dim hover:bg-surface-2 focus-visible:outline-2 focus-visible:outline-accent"
      @click="close"
    >
      <X :size="14" aria-hidden="true" />
    </button>
  </div>
</template>
