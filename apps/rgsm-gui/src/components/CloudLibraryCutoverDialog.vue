<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { commands, type CloudLibraryCutoverReview } from '~/api/commands';
import { $t } from '~/i18n';
import { LoaderCircle } from '@lucide/vue';
import { KButton, KDialog } from '../ui/kit';

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
  <KDialog
    v-model:open="visible"
    :title="
      props.resumable
        ? $t('sync_settings.library.cutover.resume_title')
        : $t('sync_settings.library.cutover.title')
    "
    :width="560"
    :dismissable="!running"
  >
    <div v-if="loading || running" class="flex justify-center py-6 text-text-dim">
      <LoaderCircle :size="22" class="animate-spin" aria-hidden="true" />
    </div>
    <template v-else-if="review">
      <p class="text-sm leading-relaxed text-text">
        {{
          props.resumable
            ? $t('sync_settings.library.cutover.resume_story')
            : $t('sync_settings.library.cutover.story')
        }}
      </p>
      <template v-if="!props.resumable">
        <p class="mt-4 text-sm font-medium text-text">
          {{ $t('sync_settings.library.cutover.after_title') }}
        </p>
        <ul class="mt-2 list-disc pl-5 text-sm leading-relaxed text-text-dim">
          <li>{{ $t('sync_settings.library.cutover.after_this_device') }}</li>
          <li>{{ $t('sync_settings.library.cutover.after_old_clients') }}</li>
          <li>{{ $t('sync_settings.library.cutover.after_damage') }}</li>
          <li>{{ $t('sync_settings.library.cutover.after_resume') }}</li>
        </ul>
      </template>
      <p v-if="running" class="mt-3 text-sm text-text-dim">
        {{ $t('sync_settings.library.cutover.running') }}
      </p>
    </template>

    <template #footer>
      <KButton :disabled="running" @click="visible = false">
        {{ $t('sync_settings.cancel') }}
      </KButton>
      <KButton variant="primary" :loading="running" :disabled="!review" @click="submit">
        {{
          props.resumable
            ? $t('sync_settings.library.cutover.resume_start')
            : $t('sync_settings.library.cutover.start')
        }}
      </KButton>
    </template>
  </KDialog>
</template>
