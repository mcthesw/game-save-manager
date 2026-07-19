<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { Monitor, MostlyCloudy } from '@element-plus/icons-vue';

import {
  commands,
  type ProgressRelation,
  type RemoteProgressCandidate,
  type V2ConflictReview,
} from '../bindings';
import { notifyError } from '../composables/useActivityCenter';
import { $t } from '../i18n';

const props = defineProps<{
  modelValue: boolean;
  gameId: string;
  gameName: string;
}>();

const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
}>();

const review = ref<V2ConflictReview | null>(null);
const loading = ref(false);

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

function relationType(relation: ProgressRelation) {
  if (relation === 'same') return 'success';
  if (relation === 'remote_ahead') return 'primary';
  if (relation === 'remote_earlier') return 'info';
  return 'warning';
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

watch(
  () => props.modelValue,
  (open) => {
    if (open) void load();
  }
);
</script>

<template>
  <ElDialog
    v-model="visible"
    :title="$t('sync_settings.archives.progress.title', { game: gameName })"
    width="min(780px, 94vw)"
    destroy-on-close
  >
    <div v-loading="loading" class="progress-review">
      <ElAlert
        v-if="review"
        :type="review.requires_choice ? 'warning' : 'success'"
        :title="
          review.requires_choice
            ? $t('sync_settings.archives.progress.choice_required')
            : $t('sync_settings.archives.progress.aligned')
        "
        :closable="false"
        show-icon
      />

      <section v-if="review" class="local-card">
        <div class="position-heading">
          <span class="position-icon"
            ><ElIcon><Monitor /></ElIcon
          ></span>
          <div>
            <small>{{ $t('sync_settings.archives.progress.local_title') }}</small>
            <strong v-if="review.local">
              {{ snapshotLabel(review.local.description, review.local.snapshot_id) }}
            </strong>
            <strong v-else>{{ $t('sync_settings.archives.progress.no_local') }}</strong>
          </div>
        </div>
        <div v-if="review.local" class="availability">
          <ElTag :type="review.local.local_available ? 'success' : 'info'" effect="plain" round>
            {{
              review.local.local_available
                ? $t('sync_settings.archives.progress.on_device')
                : $t('sync_settings.archives.progress.not_on_device')
            }}
          </ElTag>
          <ElTag :type="review.local.cloud_available ? 'primary' : 'info'" effect="plain" round>
            {{
              review.local.cloud_available
                ? $t('sync_settings.archives.progress.in_cloud')
                : $t('sync_settings.archives.progress.not_in_cloud')
            }}
          </ElTag>
        </div>
      </section>

      <div v-if="review?.candidates.length" class="candidate-list">
        <article
          v-for="candidate in review.candidates"
          :key="candidate.snapshot_id"
          class="candidate-card"
        >
          <div class="candidate-top">
            <div class="position-heading">
              <span class="position-icon cloud"
                ><ElIcon><MostlyCloudy /></ElIcon
              ></span>
              <div>
                <small>{{ devices(candidate) }}</small>
                <strong>{{ snapshotLabel(candidate.description, candidate.snapshot_id) }}</strong>
              </div>
            </div>
            <ElTag :type="relationType(candidate.relation)" effect="light" round>
              {{ $t(relationKeys[candidate.relation]) }}
            </ElTag>
          </div>

          <div class="diff-grid">
            <div>
              <span>{{ $t('sync_settings.archives.progress.local_unique') }}</span>
              <strong>{{ candidate.local_unique_snapshots }}</strong>
            </div>
            <div>
              <span>{{ $t('sync_settings.archives.progress.remote_unique') }}</span>
              <strong>{{ candidate.remote_unique_snapshots }}</strong>
            </div>
            <div>
              <span>{{ $t('sync_settings.archives.progress.shared_point') }}</span>
              <strong>
                {{
                  candidate.common_ancestor || $t('sync_settings.archives.progress.no_shared_point')
                }}
              </strong>
            </div>
          </div>

          <div class="availability">
            <ElTag :type="candidate.local_available ? 'success' : 'info'" effect="plain" round>
              {{
                candidate.local_available
                  ? $t('sync_settings.archives.progress.on_device')
                  : $t('sync_settings.archives.progress.not_on_device')
              }}
            </ElTag>
            <ElTag :type="candidate.cloud_available ? 'primary' : 'warning'" effect="plain" round>
              {{
                candidate.cloud_available
                  ? $t('sync_settings.archives.progress.in_cloud')
                  : $t('sync_settings.archives.progress.not_in_cloud')
              }}
            </ElTag>
          </div>
        </article>
      </div>
      <ElEmpty
        v-else-if="review"
        :description="$t('sync_settings.archives.progress.no_candidates')"
      />
    </div>

    <template #footer>
      <ElButton @click="visible = false">
        {{ $t('sync_settings.archives.progress.decide_later') }}
      </ElButton>
    </template>
  </ElDialog>
</template>

<style scoped>
.progress-review {
  min-height: 180px;
}

.local-card,
.candidate-card {
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 12px;
  background: var(--el-bg-color);
}

.local-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin: 16px 0;
  padding: 14px 16px;
}

.candidate-list {
  display: grid;
  gap: 12px;
}

.candidate-card {
  padding: 16px;
  box-shadow: var(--el-box-shadow-lighter);
}

.candidate-top,
.position-heading,
.availability {
  display: flex;
  align-items: center;
}

.candidate-top {
  justify-content: space-between;
  gap: 12px;
}

.position-heading {
  min-width: 0;
  gap: 10px;
}

.position-heading > div {
  display: grid;
  min-width: 0;
}

.position-heading small,
.diff-grid span {
  color: var(--el-text-color-secondary);
  font-size: 0.78rem;
}

.position-heading small {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.position-heading strong {
  overflow: hidden;
  color: var(--el-text-color-primary);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.position-icon {
  display: grid;
  width: 36px;
  height: 36px;
  flex-shrink: 0;
  place-items: center;
  border-radius: 10px;
  color: var(--el-color-success);
  background: var(--el-color-success-light-9);
}

.position-icon.cloud {
  color: var(--el-color-primary);
  background: var(--el-color-primary-light-9);
}

.diff-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
  margin: 14px 0;
}

.diff-grid div {
  display: grid;
  gap: 2px;
  min-width: 0;
  padding: 9px 10px;
  border-radius: 8px;
  background: var(--el-fill-color-light);
}

.diff-grid strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.availability {
  flex-wrap: wrap;
  gap: 8px;
}

@media (max-width: 600px) {
  .local-card,
  .candidate-top {
    align-items: flex-start;
    flex-direction: column;
  }

  .diff-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .diff-grid div:last-child {
    grid-column: 1 / -1;
  }
}
</style>
