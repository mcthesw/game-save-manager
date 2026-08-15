<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import ProcessSelect from './ProcessSelect.vue';
import { commands } from '../api/commands';
import type {
  AutoBackupConfig,
  CloudArchiveGameView,
  Game,
  RunningProcessOption,
} from '../api/commands';
import { $t } from '../i18n';
import { error } from '../utils/logger';
import { KButton, KCheckbox, KDrawer, KNumberInput, KSelect, KSwitch } from '../ui/kit';

const props = defineProps<{
  modelValue: boolean;
  game: Game;
  cloudGame?: CloudArchiveGameView | null;
}>();

const emit = defineEmits<{
  (event: 'update:modelValue', value: boolean): void;
  (event: 'saved'): void;
}>();

const { config, refreshConfig } = useConfig();
const feedback = useFeedback();

const visible = computed({
  get: () => props.modelValue,
  set: (value: boolean) => emit('update:modelValue', value),
});

const saving = ref(false);
const loadingTargets = ref(false);
const processOptions = ref<RunningProcessOption[]>([]);

const intervalPresets = [
  { label: () => $t('manage.preset_15s'), value: 15 },
  { label: () => $t('manage.preset_30s'), value: 30 },
  { label: () => $t('manage.preset_1m'), value: 60 },
  { label: () => $t('manage.preset_2m'), value: 120 },
  { label: () => $t('manage.preset_5m'), value: 300 },
  { label: () => $t('manage.preset_10m'), value: 600 },
  { label: () => $t('manage.preset_30m'), value: 1800 },
  { label: () => $t('manage.preset_1h'), value: 3600 },
];

const presetOptions = computed(() => [
  ...intervalPresets.map((preset) => ({ value: String(preset.value), label: preset.label() })),
  { value: 'custom', label: $t('manage.auto_backup_custom_interval') },
]);

const draft = reactive({
  timerEnabled: false,
  timerIntervalSecs: 300 as number | undefined,
  timerMaxCount: undefined as number | undefined,
  timerPreset: '300',
  processEnabled: false,
  processName: '',
  onStart: false,
  onExit: true,
  intervalEnabled: false,
  processIntervalSecs: 300 as number | undefined,
});
const sharedRetentionEnabled = ref(false);
const sharedRetentionLimit = ref<number | undefined>(10);

function gameIdentity(game: Game): string {
  return game.storage_key || game.name;
}

function syncDraft() {
  const timer = props.game.auto_backup;
  draft.timerEnabled = Boolean(timer);
  draft.timerIntervalSecs = timer?.interval_secs ?? 300;
  draft.timerMaxCount = timer?.max_backup_count ?? undefined;
  const matchedPreset = intervalPresets.find((preset) => preset.value === draft.timerIntervalSecs);
  draft.timerPreset = matchedPreset ? String(draft.timerIntervalSecs) : 'custom';

  const automation = findGameAutomation(config.value, props.game);
  draft.processEnabled = Boolean(
    automation?.on_process_start ||
    automation?.on_process_exit ||
    automation?.in_process_interval_secs != null
  );
  draft.processName = automation?.process_name ?? '';
  draft.onStart = automation?.on_process_start ?? false;
  draft.onExit = automation?.on_process_exit ?? true;
  draft.intervalEnabled = automation?.in_process_interval_secs != null;
  draft.processIntervalSecs = automation?.in_process_interval_secs ?? 300;
  sharedRetentionEnabled.value = props.cloudGame?.retention_limit != null;
  sharedRetentionLimit.value = props.cloudGame?.retention_limit ?? 10;
}

async function refreshTargets() {
  if (loadingTargets.value) return;
  loadingTargets.value = true;
  try {
    const processes = await commands.listRunningProcesses();
    if (processes.status === 'ok') {
      processOptions.value = processes.data;
    }
  } catch (e) {
    error(`Failed to refresh auto-save targets: ${e}`);
  } finally {
    loadingTargets.value = false;
  }
}

function onTimerPresetChange(value: string) {
  if (value !== 'custom') {
    draft.timerIntervalSecs = Number(value);
  }
}

function buildAutomation() {
  const hasProcessTrigger =
    draft.processEnabled && (draft.onStart || draft.onExit || draft.intervalEnabled);
  if (!hasProcessTrigger) {
    return null;
  }

  return {
    process_name: draft.processName,
    on_process_start: draft.processEnabled && draft.onStart,
    on_process_exit: draft.processEnabled && draft.onExit,
    in_process_interval_secs:
      draft.processEnabled && draft.intervalEnabled ? (draft.processIntervalSecs ?? 300) : null,
  };
}

async function saveDraft() {
  saving.value = true;
  try {
    const timerConfig: AutoBackupConfig | null = draft.timerEnabled
      ? {
          interval_secs: draft.timerIntervalSecs ?? 300,
          max_backup_count: draft.timerMaxCount ?? null,
        }
      : null;
    let nextRetention: number | null = null;
    let riskyRetention = false;
    if (props.cloudGame) {
      nextRetention = sharedRetentionEnabled.value
        ? Math.max(1, sharedRetentionLimit.value ?? 1)
        : null;
      const previous = props.cloudGame.retention_limit ?? null;
      riskyRetention = nextRetention !== null && (previous === null || nextRetention < previous);
      if (riskyRetention) {
        try {
          await feedback.confirm(
            $t('sync_settings.archives.retention.confirm', { count: nextRetention }),
            $t('sync_settings.archives.retention.confirm_title'),
            {
              confirmButtonText: $t('sync_settings.archives.retention.enable'),
              cancelButtonText: $t('sync_settings.cancel'),
              type: 'warning',
            }
          );
        } catch {
          return;
        }
      }
    }

    const result = await commands.setGameAutoSaveSettings(
      gameIdentity(props.game),
      timerConfig,
      buildAutomation()
    );
    if (result.status === 'error') {
      notifyError(result.error);
      return;
    }

    if (props.cloudGame) {
      const retention = await commands.setSharedSnapshotRetention(
        props.cloudGame.game_id,
        nextRetention,
        riskyRetention
      );
      if (retention.status === 'error') {
        notifyError($t('sync_settings.archives.retention.save_failed'), retention.error);
        return;
      }
    }

    await refreshConfig();
    emit('saved');
    visible.value = false;
    notifySuccess($t('manage.auto_save_settings_save_success'));
  } finally {
    saving.value = false;
  }
}

watch(
  () => visible.value,
  async (isVisible) => {
    if (!isVisible) return;
    syncDraft();
    await refreshTargets();
    syncDraft();
  }
);

watch(
  () => [props.game.name, props.game.storage_key, config.value.quick_action?.game_automations],
  () => {
    if (visible.value) {
      syncDraft();
    }
  }
);
</script>

<template>
  <KDrawer v-model:open="visible" :title="$t('manage.auto_save_settings')" :width="520">
    <div class="flex flex-col gap-3.5">
      <section class="rounded-md border border-border bg-surface p-4">
        <div class="flex items-start justify-between gap-4">
          <div class="min-w-0">
            <h3 class="text-sm font-semibold text-text">{{ $t('manage.auto_backup') }}</h3>
            <p class="mt-1 text-xs leading-relaxed text-text-dim">
              {{ $t('manage.auto_backup_timer_summary') }}
            </p>
          </div>
          <KSwitch v-model="draft.timerEnabled" />
        </div>
        <div
          v-if="draft.timerEnabled"
          class="mt-3.5 grid grid-cols-[8.75rem_minmax(0,1fr)] items-center gap-x-3 gap-y-2.5"
        >
          <span class="text-xs text-text-dim">{{ $t('manage.auto_backup_interval') }}</span>
          <KSelect
            v-model="draft.timerPreset"
            :options="presetOptions"
            size="sm"
            class="w-full"
            :aria-label="$t('manage.auto_backup_interval')"
            @update:model-value="onTimerPresetChange(String($event))"
          />
          <template v-if="draft.timerPreset === 'custom'">
            <span class="text-xs text-text-dim">{{
              $t('manage.process_monitor_interval_secs')
            }}</span>
            <KNumberInput
              v-model="draft.timerIntervalSecs"
              :min="1"
              :max="86400"
              class="w-36"
              :aria-label="$t('manage.process_monitor_interval_secs')"
            />
          </template>
          <span class="text-xs text-text-dim">{{ $t('manage.auto_backup_max_count') }}</span>
          <KNumberInput
            v-model="draft.timerMaxCount"
            :min="0"
            :max="9999"
            class="w-36"
            :placeholder="$t('manage.auto_backup_max_count_hint')"
            :aria-label="$t('manage.auto_backup_max_count')"
          />
        </div>
        <div
          v-if="cloudGame"
          class="mt-4 flex items-start justify-between gap-4 border-t border-border pt-4"
        >
          <div class="min-w-0">
            <h3 class="text-sm font-semibold text-text">
              {{ $t('manage.shared_retention_limit') }}
            </h3>
            <p class="mt-1 text-xs leading-relaxed text-text-dim">
              {{ $t('manage.shared_retention_hint') }}
            </p>
          </div>
          <div class="flex shrink-0 items-center gap-2.5">
            <KSwitch v-model="sharedRetentionEnabled" />
            <KNumberInput
              v-if="sharedRetentionEnabled"
              v-model="sharedRetentionLimit"
              :min="1"
              :max="1000"
              class="w-24"
              :aria-label="$t('manage.shared_retention_limit')"
            />
          </div>
        </div>
      </section>

      <section class="rounded-md border border-border bg-surface p-4">
        <div class="flex items-start justify-between gap-4">
          <div class="min-w-0">
            <h3 class="text-sm font-semibold text-text">{{ $t('manage.process_monitor') }}</h3>
            <p class="mt-1 text-xs leading-relaxed text-text-dim">
              {{ $t('manage.process_monitor_summary') }}
            </p>
          </div>
          <KSwitch v-model="draft.processEnabled" />
        </div>
        <div v-if="draft.processEnabled" class="mt-3.5 flex flex-col gap-3">
          <div class="grid grid-cols-[8.75rem_minmax(0,1fr)] items-center gap-3">
            <span class="text-xs text-text-dim">{{ $t('manage.process_monitor_name') }}</span>
            <ProcessSelect
              v-model="draft.processName"
              :options="processOptions"
              :loading="loadingTargets"
              :placeholder="$t('manage.process_monitor_name_placeholder')"
              @refresh="refreshTargets"
            />
          </div>
          <div class="grid grid-cols-3 gap-2">
            <KCheckbox v-model="draft.onStart">{{
              $t('manage.process_monitor_on_start')
            }}</KCheckbox>
            <KCheckbox v-model="draft.onExit">{{ $t('manage.process_monitor_on_exit') }}</KCheckbox>
            <KCheckbox v-model="draft.intervalEnabled">
              {{ $t('manage.process_monitor_interval') }}
            </KCheckbox>
          </div>
          <div
            v-if="draft.intervalEnabled"
            class="grid max-w-80 grid-cols-[8.75rem_minmax(0,1fr)] items-center gap-3"
          >
            <span class="text-xs text-text-dim">{{
              $t('manage.process_monitor_interval_secs')
            }}</span>
            <KNumberInput
              v-model="draft.processIntervalSecs"
              :min="1"
              :max="86400"
              class="w-36"
              :aria-label="$t('manage.process_monitor_interval_secs')"
            />
          </div>
        </div>
      </section>

      <p class="text-xs leading-relaxed text-text-dim">
        {{ $t('manage.auto_save_feedback_hint') }}
      </p>
    </div>

    <template #footer>
      <div class="flex justify-end gap-2.5">
        <KButton @click="visible = false">{{ $t('manage.cancel') }}</KButton>
        <KButton variant="primary" :loading="saving" @click="saveDraft">
          {{ $t('manage.save_settings') }}
        </KButton>
      </div>
    </template>
  </KDrawer>
</template>
