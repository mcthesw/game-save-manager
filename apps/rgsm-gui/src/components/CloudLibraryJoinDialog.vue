<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import {
  commands,
  type CloudLibraryJoinItem,
  type CloudLibraryJoinReview,
  type JoinGameAction,
  type JoinGameDecision,
} from '~/bindings';
import { $t } from '~/i18n';
import { LAYER } from '~/ui/layers';

const props = defineProps<{ modelValue: boolean }>();
const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'joined', gameCount: number): void;
}>();

const feedback = useFeedback();
const review = ref<CloudLibraryJoinReview | null>(null);
const actions = ref<Record<string, JoinGameAction>>({});
const selectedId = ref('');
const loading = ref(false);
const joining = ref(false);
const tagTypes = {
  same: 'success',
  local_only: 'info',
  possible_duplicate: 'warning',
  game_definition_conflict: 'danger',
} as const;

const visible = computed({
  get: () => props.modelValue,
  set: (value) => emit('update:modelValue', value),
});
const selected = computed(
  () => review.value?.items.find((item) => item.local_game_id === selectedId.value) ?? null
);
const replacementCount = computed(
  () => Object.values(actions.value).filter((action) => action === 'replace_cloud').length
);
const changedCount = computed(
  () => review.value?.items.filter((item) => item.classification !== 'same').length ?? 0
);

function classificationLabel(item: CloudLibraryJoinItem) {
  return $t(`sync_settings.library.join.classification.${item.classification}`);
}

function classificationType(item: CloudLibraryJoinItem) {
  return tagTypes[item.classification];
}

function cloudName(item: CloudLibraryJoinItem) {
  return item.cloud_names.length
    ? item.cloud_names.join(', ')
    : $t('sync_settings.library.join.not_in_cloud');
}

function recognitionLabel(value: boolean) {
  return value
    ? $t('sync_settings.library.join.recognition_available')
    : $t('sync_settings.library.join.recognition_unavailable');
}

async function loadReview() {
  loading.value = true;
  try {
    const result = await commands.reviewCloudLibraryJoin();
    if (result.status === 'error') {
      notifyError(`${$t('sync_settings.library.join.review_failed')}: ${result.error}`);
      visible.value = false;
      return;
    }
    review.value = result.data;
    actions.value = Object.fromEntries(
      result.data.items.map((item) => [item.local_game_id, 'keep_cloud'])
    );
    selectedId.value =
      result.data.items.find((item) => item.classification !== 'same')?.local_game_id ??
      result.data.items[0]?.local_game_id ??
      '';
  } catch (reason) {
    notifyError(`${$t('sync_settings.library.join.review_failed')}: ${String(reason)}`);
    visible.value = false;
  } finally {
    loading.value = false;
  }
}

function decisions(): JoinGameDecision[] {
  return (review.value?.items ?? [])
    .filter((item) => item.classification !== 'same')
    .map((item) => ({
      local_game_id: item.local_game_id,
      local_fingerprint: item.local_fingerprint,
      cloud_fingerprint: item.cloud_fingerprint,
      action: actions.value[item.local_game_id] ?? 'keep_cloud',
    }));
}

async function submit() {
  if (!review.value || joining.value) return;
  if (replacementCount.value > 0) {
    try {
      await feedback.confirm(
        $t('sync_settings.library.join.replace_warning', { count: replacementCount.value }),
        $t('sync_settings.library.join.replace_title'),
        {
          confirmButtonText: $t('sync_settings.library.join.replace_confirm'),
          cancelButtonText: $t('sync_settings.cancel'),
          type: 'warning',
        }
      );
    } catch {
      return;
    }
  }

  joining.value = true;
  try {
    const result = await commands.joinCloudLibrary(decisions(), replacementCount.value > 0);
    if (result.status === 'error') {
      notifyError(`${$t('sync_settings.library.join.join_failed')}: ${result.error}`);
      return;
    }
    if (result.data.kind === 'review_changed') {
      notifyWarning(
        $t('sync_settings.library.join.review_changed', { game: result.data.game_name })
      );
      await loadReview();
      return;
    }
    notifySuccess($t('sync_settings.library.join.join_success'));
    emit('joined', result.data.game_count);
    visible.value = false;
  } catch (reason) {
    notifyError(`${$t('sync_settings.library.join.join_failed')}: ${String(reason)}`);
  } finally {
    joining.value = false;
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
    :title="$t('sync_settings.library.join.title')"
    width="min(920px, 94vw)"
    destroy-on-close
    class="join-dialog"
    :z-index="LAYER.dialog"
  >
    <div v-loading="loading" class="join-body">
      <ElAlert
        type="warning"
        :title="$t('sync_settings.library.join.risk_title')"
        :description="$t('sync_settings.library.join.risk_description')"
        :closable="false"
        show-icon
      />

      <template v-if="review">
        <p class="join-summary">
          {{
            $t('sync_settings.library.join.summary', {
              cloud: review.cloud_game_count,
              review: changedCount,
            })
          }}
        </p>

        <div v-if="review.items.length" class="join-grid">
          <nav class="game-list" :aria-label="$t('sync_settings.library.join.games_label')">
            <ElButton
              v-for="item in review.items"
              :key="item.local_game_id"
              :type="selectedId === item.local_game_id ? 'primary' : 'default'"
              plain
              class="game-item"
              @click="selectedId = item.local_game_id"
            >
              <span class="game-name">{{ item.local_name }}</span>
              <ElTag :type="classificationType(item)" size="small">
                {{ classificationLabel(item) }}
              </ElTag>
            </ElButton>
          </nav>

          <section v-if="selected" class="game-review">
            <div class="game-review-heading">
              <h4>{{ selected.local_name }}</h4>
              <ElTag :type="classificationType(selected)">
                {{ classificationLabel(selected) }}
              </ElTag>
            </div>

            <div class="difference-table">
              <div class="difference-head">
                <span></span>
                <strong>{{ $t('sync_settings.library.join.local') }}</strong>
                <strong>{{ $t('sync_settings.library.join.cloud') }}</strong>
              </div>
              <div v-if="selected.difference.name_changed || !selected.cloud_names.length">
                <span>{{ $t('sync_settings.library.join.game_name') }}</span>
                <span>{{ selected.local_name }}</span>
                <span>{{ cloudName(selected) }}</span>
              </div>
              <div v-if="selected.difference.save_units_changed">
                <span>{{ $t('sync_settings.library.join.save_units') }}</span>
                <span>
                  {{
                    $t('sync_settings.library.join.item_count', {
                      count: selected.difference.local_save_unit_count,
                    })
                  }}
                </span>
                <span>
                  {{
                    $t('sync_settings.library.join.item_count', {
                      count: selected.difference.cloud_save_unit_count,
                    })
                  }}
                </span>
              </div>
              <div v-if="selected.difference.recognition_changed">
                <span>{{ $t('sync_settings.library.join.recognition') }}</span>
                <span>{{ recognitionLabel(selected.difference.local_recognition) }}</span>
                <span>{{ recognitionLabel(selected.difference.cloud_recognition) }}</span>
              </div>
              <p
                v-if="
                  !selected.difference.name_changed &&
                  !selected.difference.save_units_changed &&
                  !selected.difference.recognition_changed
                "
                class="no-difference"
              >
                {{ $t('sync_settings.library.join.no_difference') }}
              </p>
            </div>

            <ElRadioGroup
              v-if="selected.classification !== 'same'"
              v-model="actions[selected.local_game_id]"
              class="decision-group"
            >
              <ElRadio value="keep_cloud">
                {{ $t('sync_settings.library.join.keep_cloud') }}
              </ElRadio>
              <ElRadio
                v-if="
                  selected.classification === 'local_only' ||
                  selected.classification === 'possible_duplicate'
                "
                value="add_local"
              >
                {{ $t('sync_settings.library.join.add_local') }}
              </ElRadio>
              <ElRadio
                v-if="selected.classification === 'game_definition_conflict'"
                value="replace_cloud"
              >
                {{ $t('sync_settings.library.join.replace_cloud') }}
              </ElRadio>
            </ElRadioGroup>
          </section>
        </div>
        <ElEmpty v-else :description="$t('sync_settings.library.join.no_local_games')" />
      </template>
    </div>

    <template #footer>
      <ElButton @click="visible = false">{{ $t('sync_settings.cancel') }}</ElButton>
      <ElButton type="primary" :loading="joining" :disabled="loading || !review" @click="submit">
        {{ $t('sync_settings.library.join.join_action') }}
      </ElButton>
    </template>
  </ElDialog>
</template>

<style scoped>
:global(.join-dialog) {
  display: flex;
  flex-direction: column;
  max-height: calc(100vh - 32px);
  margin: 16px auto !important;
}

:global(.join-dialog .el-dialog__body) {
  min-height: 0;
  overflow-y: auto;
}

.join-body {
  min-height: 260px;
}

.join-summary {
  margin: 16px 0;
  color: var(--el-text-color-secondary);
}

.join-grid {
  display: grid;
  grid-template-columns: minmax(210px, 0.75fr) minmax(0, 1.7fr);
  gap: 16px;
}

.game-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  max-height: 430px;
  overflow-x: hidden;
  overflow-y: auto;
}

.game-item {
  justify-content: space-between;
  width: 100%;
  height: auto;
  margin: 0;
  white-space: normal;
}

.game-name {
  min-width: 0;
  flex: 1;
  overflow-wrap: anywhere;
  text-align: left;
}

.game-review {
  min-width: 0;
  padding: 16px;
  border: 1px solid var(--el-border-color-light);
  border-radius: var(--el-border-radius-base);
}

.game-review-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.game-review-heading h4 {
  margin: 0;
}

.difference-table {
  margin-top: 16px;
  border-top: 1px solid var(--el-border-color-lighter);
}

.difference-table > div {
  display: grid;
  grid-template-columns: minmax(90px, 0.7fr) repeat(2, minmax(0, 1fr));
  gap: 10px;
  padding: 10px 0;
  border-bottom: 1px solid var(--el-border-color-lighter);
}

.difference-table > div > span {
  overflow-wrap: anywhere;
}

.difference-table > div > span:first-child,
.no-difference {
  color: var(--el-text-color-secondary);
}

.decision-group {
  display: flex;
  align-items: flex-start;
  margin-top: 16px;
}

@media (max-width: 720px) {
  :global(.join-dialog .el-dialog__footer) {
    padding-right: calc(var(--el-dialog-padding-primary) + var(--el-component-size-large));
  }

  .join-grid {
    grid-template-columns: 1fr;
  }

  .game-list {
    max-height: 180px;
  }
}
</style>
