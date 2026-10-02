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

const { config, deviceGameStatuses, refreshConfig } = useConfig();
const sharedStatus = computed(() =>
  deviceGameStatuses.value.find((item) => item.game_id === gameIdentity(props.game))
);
const isShared = computed(
  () =>
    sharedStatus.value?.shared ??
    Boolean(props.cloudGame && !props.cloudGame.local_only && !props.cloudGame.definition_conflict)
);
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
  retentionMode: 'global',
  localLimit: 10 as number | undefined,
  localUnlimited: false,
  timerPreset: '300',
  processEnabled: false,
  processName: '',
  onStart: false,
  onExit: true,
  intervalEnabled: false,
  processIntervalSecs: 300 as number | undefined,
});
const originalLocal = ref('');
const originalShared = ref<number | null>(null);
const globalLimit = computed(() => config.value.settings.max_auto_backup_count);
const retentionOptions = computed(() => [
  {
    value: 'global',
    label: $t('manage.retention_global', {
      count: globalLimit.value === 0 ? $t('manage.retention_unlimited') : globalLimit.value,
    }),
  },
  { value: 'custom', label: $t('manage.retention_custom') },
]);
const localLimited = computed({
  get: () => (draft.retentionMode === 'global' ? globalLimit.value !== 0 : !draft.localUnlimited),
  set: (value: boolean) => {
    draft.localUnlimited = !value;
  },
});
const displayedLocalLimit = computed({
  get: () =>
    draft.retentionMode === 'global'
      ? globalLimit.value || undefined
      : draft.localUnlimited
        ? undefined
        : draft.localLimit,
  set: (value: number | undefined) => {
    draft.localLimit = value;
  },
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
  const limit = props.game.auto_backup_limit ?? timer?.max_backup_count;
  draft.retentionMode = limit == null ? 'global' : 'custom';
  draft.localUnlimited = limit === 0;
  draft.localLimit = limit || 10;
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
  originalShared.value = sharedStatus.value
    ? (sharedStatus.value.retention_limit ?? null)
    : (props.cloudGame?.retention_limit ?? null);
  sharedRetentionEnabled.value = originalShared.value !== null;
  sharedRetentionLimit.value = originalShared.value ?? 10;
  originalLocal.value = JSON.stringify(localSettings());
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

function localSettings() {
  const timer: AutoBackupConfig | null = draft.timerEnabled
    ? { interval_secs: draft.timerIntervalSecs ?? 300, max_backup_count: null }
    : null;
  const limit =
    draft.retentionMode === 'global' ? null : draft.localUnlimited ? 0 : (draft.localLimit ?? 10);
  return { timer, limit, automation: buildAutomation() };
}

async function saveDraft() {
  saving.value = true;
  try {
    const local = localSettings();
    const nextRetention = sharedRetentionEnabled.value
      ? Math.max(1, sharedRetentionLimit.value ?? 1)
      : null;
    const sharedChanged = isShared.value && nextRetention !== originalShared.value;
    const riskyRetention =
      sharedChanged &&
      nextRetention !== null &&
      (originalShared.value === null || nextRetention < originalShared.value);
    if (riskyRetention) {
      try {
        await feedback.confirm(
          $t('sync_settings.archives.retention.confirm', { count: nextRetention }),
          $t('sync_settings.archives.retention.confirm_title'),
          {
            confirmButtonText: $t('sync_settings.archives.retention.enable'),
            cancelButtonText: $t('manage.cancel'),
            type: 'warning',
          }
        );
      } catch {
        return;
      }
    }
    if (JSON.stringify(local) !== originalLocal.value) {
      const result = await commands.setGameAutoSaveSettings(
        gameIdentity(props.game),
        local.timer,
        local.automation,
        local.limit
      );
      if (result.status === 'error') {
        notifyError(result.error);
        return;
      }
      originalLocal.value = JSON.stringify(local);
      await refreshConfig();
      emit('saved');
    }
    if (sharedChanged) {
      const result = await commands.setSharedSnapshotRetention(
        gameIdentity(props.game),
        nextRetention,
        riskyRetention
      );
      if (result.status === 'error') {
        notifyError($t('manage.retention_shared_save_failed'), result.error);
        return;
      }
      originalShared.value = nextRetention;
      await refreshConfig();
      emit('saved');
    }
    visible.value = false;
    notifySuccess($t('manage.auto_save_settings_save_success'));
  } finally {
    saving.value = false;
  }
}

watch(
  [() => visible.value, () => gameIdentity(props.game)],
  ([isVisible]) => {
    if (!isVisible) return;
    syncDraft();
    void refreshTargets();
  },
  { immediate: true }
);
</script>

<template>
  <KDrawer v-model:open="visible" :title="$t('manage.auto_save_settings')" :width="520">
    <div class="backup-settings">
      <section class="settings-section">
        <h2 class="section-title">{{ $t('manage.backup_when') }}</h2>
        <div class="settings-group">
          <h3>{{ $t('manage.auto_backup') }}</h3>
          <div class="form-row">
            <span>{{ $t('manage.backup_enabled') }}</span>
            <KSwitch
              v-model="draft.timerEnabled"
              class="justify-self-start"
              :aria-label="$t('manage.auto_backup')"
            />
          </div>
          <div class="form-row">
            <span>{{ $t('manage.auto_backup_interval') }}</span>
            <KSelect
              v-model="draft.timerPreset"
              :options="presetOptions"
              :disabled="!draft.timerEnabled"
              class="w-full"
              :aria-label="$t('manage.auto_backup_interval')"
              @update:model-value="onTimerPresetChange(String($event))"
            />
          </div>
          <div class="form-row">
            <span>{{ $t('manage.process_monitor_interval_secs') }}</span>
            <KNumberInput
              v-model="draft.timerIntervalSecs"
              :min="1"
              :max="86400"
              :disabled="!draft.timerEnabled || draft.timerPreset !== 'custom'"
              class="w-36"
              :aria-label="$t('manage.timer_interval_seconds')"
            />
          </div>
        </div>

        <div class="settings-group">
          <h3>{{ $t('manage.process_monitor') }}</h3>
          <div class="form-row">
            <span>{{ $t('manage.backup_enabled') }}</span>
            <KSwitch
              v-model="draft.processEnabled"
              class="justify-self-start"
              :aria-label="$t('manage.process_monitor')"
            />
          </div>
          <div class="form-row">
            <span>{{ $t('manage.process_monitor_name') }}</span>
            <ProcessSelect
              v-model="draft.processName"
              :options="processOptions"
              :loading="loadingTargets"
              :disabled="!draft.processEnabled"
              :placeholder="$t('manage.process_monitor_name_placeholder')"
              @refresh="refreshTargets"
            />
          </div>
          <div class="form-row items-start">
            <span class="pt-0.5">{{ $t('manage.backup_triggers') }}</span>
            <div class="flex flex-wrap gap-x-4 gap-y-3">
              <KCheckbox v-model="draft.onStart" :disabled="!draft.processEnabled">{{
                $t('manage.process_monitor_on_start')
              }}</KCheckbox>
              <KCheckbox v-model="draft.onExit" :disabled="!draft.processEnabled">{{
                $t('manage.process_monitor_on_exit')
              }}</KCheckbox>
              <KCheckbox v-model="draft.intervalEnabled" :disabled="!draft.processEnabled">{{
                $t('manage.process_monitor_interval')
              }}</KCheckbox>
            </div>
          </div>
          <div class="form-row">
            <span>{{ $t('manage.process_monitor_interval_secs') }}</span>
            <KNumberInput
              v-model="draft.processIntervalSecs"
              :min="1"
              :max="86400"
              :disabled="!draft.processEnabled || !draft.intervalEnabled"
              class="w-36"
              :aria-label="$t('manage.process_monitor_interval_secs')"
            />
          </div>
        </div>
      </section>

      <section class="settings-section">
        <h2 class="section-title">{{ $t('manage.backup_retention') }}</h2>
        <div class="settings-group">
          <h3>{{ $t('manage.retention_local') }}</h3>
          <div class="form-row">
            <span>{{ $t('manage.retention_source') }}</span>
            <KSelect
              v-model="draft.retentionMode"
              :options="retentionOptions"
              class="w-full"
              :aria-label="$t('manage.retention_local')"
            />
          </div>
          <div class="form-row">
            <span>{{ $t('manage.retention_limit_enabled') }}</span>
            <KSwitch
              v-model="localLimited"
              class="justify-self-start"
              :disabled="draft.retentionMode === 'global'"
              :aria-label="$t('manage.retention_local_limit_enabled')"
            />
          </div>
          <div class="form-row">
            <span>{{ $t('manage.retention_count') }}</span>
            <div class="flex items-center gap-2">
              <KNumberInput
                v-model="displayedLocalLimit"
                :min="1"
                :max="9999"
                :disabled="draft.retentionMode === 'global' || !localLimited"
                :placeholder="$t('manage.retention_unlimited')"
                class="w-36"
                :aria-label="$t('manage.auto_backup_max_count')"
              />
              <span class="text-text-dim">{{ $t('manage.retention_count_unit') }}</span>
            </div>
          </div>
          <p class="setting-hint">{{ $t('manage.retention_local_hint') }}</p>
        </div>

        <div v-if="isShared" class="settings-group">
          <h3>{{ $t('manage.shared_retention_limit') }}</h3>
          <div class="form-row">
            <span>{{ $t('manage.retention_limit_enabled') }}</span>
            <KSwitch
              v-model="sharedRetentionEnabled"
              class="justify-self-start"
              :aria-label="$t('manage.retention_shared_limit_enabled')"
            />
          </div>
          <div class="form-row">
            <span>{{ $t('manage.retention_branch_count') }}</span>
            <div class="flex items-center gap-2">
              <KNumberInput
                v-model="sharedRetentionLimit"
                :min="1"
                :max="1000"
                :disabled="!sharedRetentionEnabled"
                class="w-36"
                :aria-label="$t('manage.shared_retention_limit')"
              />
              <span class="text-text-dim">{{ $t('manage.retention_count_unit') }}</span>
            </div>
          </div>
          <p class="setting-hint">{{ $t('manage.shared_retention_hint') }}</p>
        </div>
      </section>
      <p class="setting-hint">{{ $t('manage.auto_save_feedback_hint') }}</p>
    </div>
    <template #footer>
      <div class="flex justify-end gap-2.5">
        <KButton @click="visible = false">{{ $t('manage.cancel') }}</KButton>
        <KButton variant="primary" :loading="saving" @click="saveDraft">{{
          $t('manage.save_settings')
        }}</KButton>
      </div>
    </template>
  </KDrawer>
</template>

<style scoped>
.backup-settings {
  display: flex;
  flex-direction: column;
  gap: 28px;
  font-size: 14px;
}
.settings-section {
  display: flex;
  flex-direction: column;
  gap: 20px;
}
.section-title {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  color: var(--color-text);
}
.settings-group {
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.settings-group + .settings-group {
  border-top: 1px solid var(--color-border);
  padding-top: 20px;
}
.settings-group h3 {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
}
.form-row {
  display: grid;
  grid-template-columns: 120px minmax(0, 1fr);
  align-items: center;
  gap: 12px;
}
.setting-hint {
  margin: 0;
  color: var(--color-text-dim);
  font-size: 12px;
  line-height: 1.65;
}
@media (max-width: 420px) {
  .form-row {
    grid-template-columns: 100px minmax(0, 1fr);
    gap: 8px;
  }
}
</style>
