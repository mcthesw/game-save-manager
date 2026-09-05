<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import {
  commands,
  type CloudLibraryJoinItem,
  type CloudLibraryJoinReview,
  type JoinGameAction,
  type JoinGameDecision,
} from '~/api/commands';
import { $t } from '~/i18n';
import { CheckCircle2, Inbox, LoaderCircle } from '@lucide/vue';
import { KAlert, KButton, KDialog, KTag } from '../ui/kit';

const props = defineProps<{ modelValue: boolean; gameId?: string }>();
const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'joined', gameCount: number): void;
}>();

const feedback = useFeedback();
const review = ref<CloudLibraryJoinReview | null>(null);
const actions = ref(new Map<string, JoinGameAction>());
const selectedId = ref('');
const loading = ref(false);
const joining = ref(false);
const tagTones = {
  same: 'success',
  local_only: 'neutral',
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
  () => [...actions.value.values()].filter((action) => action === 'replace_cloud').length
);
const changedCount = computed(
  () => review.value?.items.filter((item) => item.classification !== 'same').length ?? 0
);
const reviewItems = computed(
  () => review.value?.items.filter((item) => item.classification !== 'same') ?? []
);
const unresolvedCount = computed(
  () =>
    review.value?.items.filter(
      (item) =>
        item.classification === 'game_definition_conflict' && !actions.value.has(item.local_game_id)
    ).length ?? 0
);

function classificationLabel(item: CloudLibraryJoinItem) {
  return $t(`sync_settings.library.join.classification.${item.classification}`);
}

function classificationTone(item: CloudLibraryJoinItem) {
  return tagTones[item.classification];
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
    review.value = {
      ...result.data,
      items: props.gameId
        ? result.data.items.filter((item) => item.local_game_id === props.gameId)
        : result.data.items,
    };
    actions.value = new Map<string, JoinGameAction>(
      review.value.items
        .filter((item) => item.classification !== 'game_definition_conflict')
        .map((item) => [item.local_game_id, 'keep_cloud'])
    );
    selectedId.value =
      review.value.items.find((item) => item.classification !== 'same')?.local_game_id ??
      review.value.items[0]?.local_game_id ??
      '';
  } catch (reason) {
    notifyError(`${$t('sync_settings.library.join.review_failed')}: ${String(reason)}`);
    visible.value = false;
  } finally {
    loading.value = false;
  }
}

function decisionOptions(item: CloudLibraryJoinItem): { value: JoinGameAction; label: string }[] {
  const options: { value: JoinGameAction; label: string }[] = [
    { value: 'keep_cloud', label: $t('sync_settings.library.join.keep_cloud') },
  ];
  if (item.classification === 'local_only' || item.classification === 'possible_duplicate') {
    options.push({ value: 'add_local', label: $t('sync_settings.library.join.add_local') });
  }
  if (item.classification === 'game_definition_conflict') {
    options.push({ value: 'replace_cloud', label: $t('sync_settings.library.join.replace_cloud') });
  }
  return options;
}

function decisions(): JoinGameDecision[] {
  return (review.value?.items ?? [])
    .filter((item) => item.classification !== 'same')
    .flatMap((item) => {
      const action = actions.value.get(item.local_game_id);
      return action
        ? [
            {
              local_game_id: item.local_game_id,
              local_fingerprint: item.local_fingerprint,
              cloud_fingerprint: item.cloud_fingerprint,
              action,
            },
          ]
        : [];
    });
}

async function submit() {
  if (!review.value || joining.value || unresolvedCount.value) return;
  if (replacementCount.value > 0) {
    try {
      await feedback.confirm(
        $t('sync_settings.library.join.replace_warning', { count: replacementCount.value }),
        $t('sync_settings.library.join.replace_title'),
        {
          confirmButtonText: $t(
            props.gameId
              ? 'sync_settings.library.definitions.apply'
              : 'sync_settings.library.join.replace_confirm'
          ),
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
    notifySuccess(
      $t(
        props.gameId
          ? 'sync_settings.library.definitions.saved'
          : 'sync_settings.library.join.join_success'
      )
    );
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
  },
  { immediate: true }
);
</script>
<template>
  <KDialog
    v-model:open="visible"
    :title="
      props.gameId
        ? $t('sync_settings.library.definitions.action')
        : changedCount
          ? $t('sync_settings.library.join.title')
          : $t('sync_settings.library.join.confirm_title')
    "
    :width="changedCount ? 920 : 560"
    :dismissable="!joining"
  >
    <div v-if="loading || joining" class="flex justify-center py-6 text-text-dim">
      <LoaderCircle :size="22" class="animate-spin" aria-hidden="true" />
    </div>
    <template v-else-if="review && !changedCount">
      <p class="text-sm leading-relaxed text-text">
        {{
          $t(
            props.gameId && review.items.length === 0
              ? 'sync_settings.library.definitions.unavailable'
              : 'sync_settings.library.join.confirm_story'
          )
        }}
      </p>
    </template>
    <template v-else>
      <p v-if="props.gameId" class="mb-3 text-sm text-text-dim">
        {{ $t('sync_settings.library.definitions.description') }}
      </p>
      <KAlert v-else tone="warning" class="mb-3">
        <strong class="mb-0.5 block">{{ $t('sync_settings.library.join.risk_title') }}</strong>
        {{ $t('sync_settings.library.join.risk_description') }}
      </KAlert>

      <template v-if="review">
        <p v-if="!props.gameId" class="mb-3 text-sm text-text-dim">
          {{
            $t('sync_settings.library.join.summary', {
              cloud: review.cloud_game_count,
              review: changedCount,
            })
          }}
        </p>

        <div v-if="reviewItems.length" class="grid grid-cols-[13rem_minmax(0,1fr)] gap-4">
          <nav
            class="flex max-h-80 flex-col gap-1 overflow-y-auto"
            :aria-label="$t('sync_settings.library.join.games_label')"
          >
            <button
              v-for="item in reviewItems"
              :key="item.local_game_id"
              type="button"
              class="flex cursor-pointer flex-col gap-1 rounded-sm border px-2.5 py-2 text-left transition-colors focus-visible:outline-2 focus-visible:outline-accent"
              :class="
                selectedId === item.local_game_id
                  ? 'border-accent bg-accent-soft'
                  : 'border-border bg-surface hover:border-border-strong'
              "
              @click="selectedId = item.local_game_id"
            >
              <span class="truncate text-sm font-medium text-text">{{ item.local_name }}</span>
              <KTag :tone="classificationTone(item)">{{ classificationLabel(item) }}</KTag>
            </button>
          </nav>

          <section v-if="selected" class="min-w-0">
            <div class="mb-2 flex items-center gap-2">
              <h4 class="truncate text-sm font-semibold text-text">{{ selected.local_name }}</h4>
              <KTag :tone="classificationTone(selected)">{{ classificationLabel(selected) }}</KTag>
            </div>

            <div class="rounded-md border border-border text-sm">
              <div
                class="grid grid-cols-[6rem_minmax(0,1fr)_minmax(0,1fr)] gap-2 border-b border-border px-3 py-1.5 text-xs font-medium text-text-dim"
              >
                <span></span>
                <strong>{{ $t('sync_settings.library.join.local') }}</strong>
                <strong>{{ $t('sync_settings.library.join.cloud') }}</strong>
              </div>
              <div
                v-if="selected.difference.name_changed || !selected.cloud_names.length"
                class="grid grid-cols-[6rem_minmax(0,1fr)_minmax(0,1fr)] gap-2 border-b border-border px-3 py-1.5"
              >
                <span class="text-xs text-text-dim">{{
                  $t('sync_settings.library.join.game_name')
                }}</span>
                <span class="truncate">{{ selected.local_name }}</span>
                <span class="truncate">{{ cloudName(selected) }}</span>
              </div>
              <div
                v-if="selected.difference.save_units_changed"
                class="grid grid-cols-[6rem_minmax(0,1fr)_minmax(0,1fr)] gap-2 border-b border-border px-3 py-1.5"
              >
                <span class="text-xs text-text-dim">{{
                  $t('sync_settings.library.join.save_units')
                }}</span>
                <span>{{
                  $t('sync_settings.library.join.item_count', {
                    count: selected.difference.local_save_unit_count,
                  })
                }}</span>
                <span>{{
                  $t('sync_settings.library.join.item_count', {
                    count: selected.difference.cloud_save_unit_count,
                  })
                }}</span>
              </div>
              <div
                v-if="selected.difference.recognition_changed"
                class="grid grid-cols-[6rem_minmax(0,1fr)_minmax(0,1fr)] gap-2 border-b border-border px-3 py-1.5"
              >
                <span class="text-xs text-text-dim">{{
                  $t('sync_settings.library.join.recognition')
                }}</span>
                <span>{{ recognitionLabel(selected.difference.local_recognition) }}</span>
                <span>{{ recognitionLabel(selected.difference.cloud_recognition) }}</span>
              </div>
              <p
                v-if="
                  !selected.difference.name_changed &&
                  !selected.difference.save_units_changed &&
                  !selected.difference.recognition_changed
                "
                class="px-3 py-2 text-xs text-text-dim"
              >
                {{ $t('sync_settings.library.join.no_difference') }}
              </p>
            </div>

            <div v-if="selected.classification !== 'same'" class="mt-3 flex flex-col gap-1.5">
              <button
                v-for="option in decisionOptions(selected)"
                :key="option.value"
                type="button"
                class="flex cursor-pointer items-center gap-2 rounded-sm border px-2.5 py-1.5 text-left text-sm transition-colors focus-visible:outline-2 focus-visible:outline-accent"
                :class="
                  actions.get(selected.local_game_id) === option.value
                    ? 'border-accent bg-accent-soft text-text'
                    : 'border-border bg-surface text-text-dim hover:border-border-strong hover:text-text'
                "
                :aria-pressed="actions.get(selected.local_game_id) === option.value"
                @click="actions.set(selected.local_game_id, option.value)"
              >
                <CheckCircle2
                  v-if="actions.get(selected.local_game_id) === option.value"
                  :size="14"
                  class="shrink-0 text-accent"
                  aria-hidden="true"
                />
                <span
                  v-else
                  class="inline-block h-3.5 w-3.5 shrink-0 rounded-full border border-border-strong"
                  aria-hidden="true"
                />
                {{ option.label }}
              </button>
            </div>
          </section>
        </div>
        <div v-else class="flex flex-col items-center gap-2 py-6 text-text-dim">
          <Inbox :size="26" aria-hidden="true" />
          <p class="text-sm">{{ $t('sync_settings.library.join.no_local_games') }}</p>
        </div>
      </template>
    </template>

    <template #footer>
      <span v-if="unresolvedCount" class="mr-auto text-xs text-text-dim">
        {{ $t('sync_settings.library.join.choice_required', { count: unresolvedCount }) }}
      </span>
      <KButton @click="visible = false">{{ $t('sync_settings.cancel') }}</KButton>
      <KButton
        variant="primary"
        :loading="joining"
        :disabled="
          loading ||
          !review ||
          unresolvedCount > 0 ||
          (Boolean(props.gameId) && review.items.length === 0)
        "
        @click="submit"
      >
        {{
          $t(
            props.gameId
              ? 'sync_settings.library.definitions.apply'
              : 'sync_settings.library.join.join_action'
          )
        }}
      </KButton>
    </template>
  </KDialog>
</template>
