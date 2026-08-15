<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { Cloud, Inbox, LoaderCircle, Monitor } from '@lucide/vue';

import {
  commands,
  type ProgressRelation,
  type RemoteProgressCandidate,
  type V2ConflictReview,
} from '../api/commands';
import { notifyError, notifySuccess } from '../composables/useActivityCenter';
import { $t } from '../i18n';
import { KAlert, KButton, KDialog, KTag } from '../ui/kit';

const props = defineProps<{
  modelValue: boolean;
  gameId: string;
  gameName: string;
}>();

const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'resolved'): void;
}>();

const feedback = useFeedback();
const review = ref<V2ConflictReview | null>(null);
const loading = ref(false);
const resolving = ref(false);
const acceptingSnapshotId = ref('');
const busy = computed(() => resolving.value || acceptingSnapshotId.value !== '');

const visible = computed({
  get: () => props.modelValue,
  set: (value: boolean) => emit('update:modelValue', value),
});

const relationKeys: Record<ProgressRelation, string> = {
  same: 'sync_settings.archives.progress.relation_same',
  remote_ahead: 'sync_settings.archives.progress.relation_ahead',
  remote_earlier: 'sync_settings.archives.progress.relation_earlier',
  different_progress: 'sync_settings.archives.progress.relation_different',
  no_local_position: 'sync_settings.archives.progress.relation_no_local',
};

function snapshotLabel(description: string, snapshotId: string) {
  return description || snapshotId;
}

function relationTone(relation: ProgressRelation) {
  if (relation === 'same') return 'success' as const;
  if (relation === 'remote_ahead') return 'accent' as const;
  if (relation === 'remote_earlier') return 'neutral' as const;
  return 'warning' as const;
}

function devices(candidate: RemoteProgressCandidate) {
  return candidate.devices.join(' · ');
}

async function load() {
  loading.value = true;
  review.value = null;
  try {
    const result = await commands.reviewV2GameProgress(props.gameId);
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.progress.load_failed'), result.error);
      return;
    }
    review.value = result.data;
  } finally {
    loading.value = false;
  }
}

async function keepLocal() {
  const current = review.value;
  if (!current?.local) return;
  try {
    await feedback.confirm(
      $t('sync_settings.archives.progress.keep_local_confirm'),
      $t('sync_settings.archives.progress.keep_local_title'),
      {
        confirmButtonText: $t('sync_settings.archives.progress.keep_local'),
        cancelButtonText: $t('sync_settings.cancel'),
        type: 'warning',
      }
    );
  } catch {
    return;
  }
  resolving.value = true;
  try {
    const result = await commands.keepV2LocalProgress(
      props.gameId,
      current.manifest_revision,
      current.local.snapshot_id
    );
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.progress.resolve_failed'), result.error);
      await load();
      return;
    }
    notifySuccess(
      $t('sync_settings.archives.progress.keep_local_success', {
        count: result.data.uploaded_archives,
      })
    );
    emit('resolved');
    visible.value = false;
  } finally {
    resolving.value = false;
  }
}

async function acceptRemote(candidate: RemoteProgressCandidate) {
  const current = review.value;
  if (!current || !candidate.cloud_available) return;
  try {
    await feedback.confirm(
      $t('sync_settings.archives.progress.accept_remote_confirm', {
        progress: snapshotLabel(candidate.description, candidate.snapshot_id),
      }),
      $t('sync_settings.archives.progress.accept_remote_title'),
      {
        confirmButtonText: $t('sync_settings.archives.progress.accept_remote'),
        cancelButtonText: $t('sync_settings.cancel'),
        type: 'warning',
      }
    );
  } catch {
    return;
  }
  acceptingSnapshotId.value = candidate.snapshot_id;
  try {
    const result = await commands.acceptV2RemoteProgress(
      props.gameId,
      current.manifest_revision,
      current.local?.snapshot_id ?? null,
      candidate.snapshot_id
    );
    if (result.status === 'error') {
      notifyError($t('sync_settings.archives.progress.resolve_failed'), result.error);
      await load();
      return;
    }
    notifySuccess(
      result.data.safety_backup_created
        ? $t('sync_settings.archives.progress.accept_remote_success_with_backup')
        : $t('sync_settings.archives.progress.accept_remote_success')
    );
    emit('resolved');
    visible.value = false;
  } finally {
    acceptingSnapshotId.value = '';
  }
}

watch(
  () => props.modelValue,
  (open) => {
    if (open) void load();
  }
);
</script>

<template>
  <KDialog
    v-model:open="visible"
    :title="$t('sync_settings.archives.progress.title', { game: gameName })"
    :width="780"
  >
    <div v-if="loading" class="flex min-h-44 items-center justify-center text-text-dim">
      <LoaderCircle :size="22" class="animate-spin" aria-hidden="true" />
    </div>
    <template v-else>
      <KAlert v-if="review" :tone="review.requires_choice ? 'warning' : 'success'" class="mb-4">
        {{
          review.requires_choice
            ? $t('sync_settings.archives.progress.choice_required')
            : $t('sync_settings.archives.progress.aligned')
        }}
      </KAlert>

      <section
        v-if="review"
        class="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-md border border-border p-3.5"
      >
        <div class="flex items-center gap-3">
          <Monitor :size="18" class="shrink-0 text-text-dim" aria-hidden="true" />
          <div class="min-w-0">
            <div class="text-xs text-text-dim">
              {{ $t('sync_settings.archives.progress.local_title') }}
            </div>
            <div class="truncate text-sm font-semibold text-text">
              <template v-if="review.local">
                {{ snapshotLabel(review.local.description, review.local.snapshot_id) }}
              </template>
              <template v-else>{{ $t('sync_settings.archives.progress.no_local') }}</template>
            </div>
          </div>
        </div>
        <div v-if="review.local" class="flex shrink-0 gap-1.5">
          <KTag :tone="review.local.local_available ? 'success' : 'neutral'">
            {{
              review.local.local_available
                ? $t('sync_settings.archives.progress.on_device')
                : $t('sync_settings.archives.progress.not_on_device')
            }}
          </KTag>
          <KTag :tone="review.local.cloud_available ? 'accent' : 'neutral'">
            {{
              review.local.cloud_available
                ? $t('sync_settings.archives.progress.in_cloud')
                : $t('sync_settings.archives.progress.not_in_cloud')
            }}
          </KTag>
        </div>
      </section>

      <div v-if="review?.candidates.length" class="flex flex-col gap-3">
        <article
          v-for="candidate in review.candidates"
          :key="candidate.snapshot_id"
          class="rounded-md border border-border p-3.5"
        >
          <div class="mb-3 flex items-start justify-between gap-3">
            <div class="flex min-w-0 items-center gap-3">
              <Cloud :size="18" class="shrink-0 text-text-dim" aria-hidden="true" />
              <div class="min-w-0">
                <div class="truncate text-xs text-text-dim">{{ devices(candidate) }}</div>
                <div class="truncate text-sm font-semibold text-text">
                  {{ snapshotLabel(candidate.description, candidate.snapshot_id) }}
                </div>
              </div>
            </div>
            <KTag :tone="relationTone(candidate.relation)" class="shrink-0">
              {{ $t(relationKeys[candidate.relation]) }}
            </KTag>
          </div>

          <div class="mb-3 grid grid-cols-3 gap-2 rounded-sm bg-surface-2 px-3 py-2">
            <div>
              <div class="text-[11px] text-text-dim">
                {{ $t('sync_settings.archives.progress.local_unique') }}
              </div>
              <div class="font-mono text-sm text-text">{{ candidate.local_unique_snapshots }}</div>
            </div>
            <div>
              <div class="text-[11px] text-text-dim">
                {{ $t('sync_settings.archives.progress.remote_unique') }}
              </div>
              <div class="font-mono text-sm text-text">{{ candidate.remote_unique_snapshots }}</div>
            </div>
            <div class="min-w-0">
              <div class="text-[11px] text-text-dim">
                {{ $t('sync_settings.archives.progress.shared_point') }}
              </div>
              <div class="truncate font-mono text-xs text-text">
                {{
                  candidate.common_ancestor || $t('sync_settings.archives.progress.no_shared_point')
                }}
              </div>
            </div>
          </div>

          <div class="flex items-center justify-between gap-3">
            <div class="flex gap-1.5">
              <KTag :tone="candidate.local_available ? 'success' : 'neutral'">
                {{
                  candidate.local_available
                    ? $t('sync_settings.archives.progress.on_device')
                    : $t('sync_settings.archives.progress.not_on_device')
                }}
              </KTag>
              <KTag :tone="candidate.cloud_available ? 'accent' : 'warning'">
                {{
                  candidate.cloud_available
                    ? $t('sync_settings.archives.progress.in_cloud')
                    : $t('sync_settings.archives.progress.not_in_cloud')
                }}
              </KTag>
            </div>
            <KButton
              v-if="candidate.relation !== 'same'"
              size="sm"
              :disabled="busy || !candidate.cloud_available"
              :loading="acceptingSnapshotId === candidate.snapshot_id"
              @click="acceptRemote(candidate)"
            >
              {{ $t('sync_settings.archives.progress.accept_remote') }}
            </KButton>
          </div>
        </article>
      </div>
      <div v-else-if="review" class="flex flex-col items-center gap-2 py-6 text-text-dim">
        <Inbox :size="26" aria-hidden="true" />
        <p class="text-sm">{{ $t('sync_settings.archives.progress.no_candidates') }}</p>
      </div>
    </template>

    <template #footer>
      <KButton :disabled="busy" @click="visible = false">
        {{ $t('sync_settings.archives.progress.decide_later') }}
      </KButton>
      <KButton
        v-if="review?.requires_choice && review.local"
        variant="primary"
        :disabled="busy"
        :loading="resolving"
        @click="keepLocal"
      >
        {{ $t('sync_settings.archives.progress.keep_local') }}
      </KButton>
    </template>
  </KDialog>
</template>
