<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { commands, type CloudLibraryCutoverReview } from '~/api/commands';
import { $t } from '~/i18n';
import { LoaderCircle } from '@lucide/vue';
import { KButton, KCheckbox, KDialog } from '../ui/kit';

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
const applyAutomaticCleanup = ref(false);
const visible = computed({
  get: () => props.modelValue,
  set: (value) => emit('update:modelValue', value),
});

async function loadReview() {
  loading.value = true;
  applyAutomaticCleanup.value = false;
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
    const result = await commands.cutoverCloudLibrary(true, applyAutomaticCleanup.value);
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
          <li>{{ $t('sync_settings.library.cutover.after_other_devices') }}</li>
          <li>{{ $t('sync_settings.library.cutover.after_damage') }}</li>
          <li>{{ $t('sync_settings.library.cutover.after_resume') }}</li>
        </ul>
      </template>
      <div v-if="review.unlisted_cloud_games.length" class="mt-4">
        <p class="text-sm font-medium text-text">
          {{
            $t('sync_settings.library.cutover.unlisted_title', {
              count: review.unlisted_cloud_games.length,
            })
          }}
        </p>
        <p class="mt-1 text-sm leading-relaxed text-text-dim">
          {{ $t('sync_settings.library.cutover.unlisted_description') }}
        </p>
        <ul class="mt-2 max-h-32 list-disc overflow-auto pl-5 text-sm text-text-dim">
          <li v-for="item in review.unlisted_cloud_games" :key="item.game_id">
            {{
              $t('sync_settings.library.cutover.unlisted_item', {
                game: item.game_id,
                count: item.snapshot_count,
              })
            }}
          </li>
        </ul>
      </div>
      <div v-if="review.local_only_games.length" class="mt-4">
        <p class="text-sm font-medium text-text">
          {{
            $t('sync_settings.library.cutover.local_only_title', {
              count: review.local_only_games.length,
            })
          }}
        </p>
        <p class="mt-1 text-sm leading-relaxed text-text-dim">
          {{ $t('sync_settings.library.cutover.local_only_description') }}
        </p>
        <ul class="mt-2 max-h-32 list-disc overflow-auto pl-5 text-sm text-text-dim">
          <li v-for="name in review.local_only_games" :key="name">{{ name }}</li>
        </ul>
      </div>
      <div v-if="review.automatic_cleanup.length" class="mt-4">
        <p class="text-sm font-medium text-text">
          {{
            $t('sync_settings.library.cutover.cleanup_title', {
              count: review.automatic_cleanup.length,
            })
          }}
        </p>
        <p class="mt-1 text-sm leading-relaxed text-text-dim">
          {{ $t('sync_settings.library.cutover.cleanup_description') }}
        </p>
        <ul class="mt-2 max-h-32 list-disc overflow-auto pl-5 text-sm text-text-dim">
          <li v-for="item in review.automatic_cleanup" :key="item.game_id">
            {{
              $t('sync_settings.library.cutover.cleanup_item', {
                game: item.game_name,
                count: item.automatic_snapshots_per_branch,
              })
            }}
          </li>
        </ul>
        <KCheckbox v-model="applyAutomaticCleanup" class="mt-3">
          {{ $t('sync_settings.library.cutover.cleanup_confirm') }}
        </KCheckbox>
      </div>
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
