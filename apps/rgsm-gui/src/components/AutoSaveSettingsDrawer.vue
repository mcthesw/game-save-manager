<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import ProcessSelect from './ProcessSelect.vue';
import { commands } from '../bindings';
import type {
  AutoBackupConfig,
  CloudArchiveGameView,
  Game,
  RunningProcessOption,
} from '../bindings';
import { $t } from '../i18n';
import { error } from '@tauri-apps/plugin-log';

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

const draft = reactive({
  timerEnabled: false,
  timerIntervalSecs: 300,
  timerMaxCount: undefined as number | undefined,
  timerPreset: '300',
  processEnabled: false,
  processName: '',
  onStart: false,
  onExit: true,
  intervalEnabled: false,
  processIntervalSecs: 300,
});
const sharedRetentionEnabled = ref(false);
const sharedRetentionLimit = ref(10);

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
      draft.processEnabled && draft.intervalEnabled ? draft.processIntervalSecs : null,
  };
}

async function saveDraft() {
  saving.value = true;
  try {
    const timerConfig: AutoBackupConfig | null = draft.timerEnabled
      ? {
          interval_secs: draft.timerIntervalSecs,
          max_backup_count: draft.timerMaxCount ?? null,
        }
      : null;
    let nextRetention: number | null = null;
    let riskyRetention = false;
    if (props.cloudGame) {
      nextRetention = sharedRetentionEnabled.value ? Math.max(1, sharedRetentionLimit.value) : null;
      const previous = props.cloudGame.retention_limit;
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
  <el-drawer
    v-model="visible"
    :title="$t('manage.auto_save_settings')"
    size="520px"
    append-to-body
    destroy-on-close
  >
    <div class="auto-save-drawer">
      <section class="settings-panel">
        <div class="panel-header">
          <div class="panel-heading">
            <h3>{{ $t('manage.auto_backup') }}</h3>
            <span>{{ $t('manage.auto_backup_timer_summary') }}</span>
          </div>
          <el-switch v-model="draft.timerEnabled" />
        </div>
        <div v-if="draft.timerEnabled" class="panel-grid">
          <span class="field-label">{{ $t('manage.auto_backup_interval') }}</span>
          <el-select
            v-model="draft.timerPreset"
            size="small"
            class="field-control"
            @change="onTimerPresetChange"
          >
            <el-option
              v-for="preset in intervalPresets"
              :key="preset.value"
              :label="preset.label()"
              :value="String(preset.value)"
            />
            <el-option :label="$t('manage.auto_backup_custom_interval')" value="custom" />
          </el-select>
          <template v-if="draft.timerPreset === 'custom'">
            <span class="field-label">{{ $t('manage.process_monitor_interval_secs') }}</span>
            <el-input-number
              v-model="draft.timerIntervalSecs"
              :min="1"
              :max="86400"
              :step="1"
              size="small"
              class="number-control"
            />
          </template>
          <span class="field-label">{{ $t('manage.auto_backup_max_count') }}</span>
          <el-input-number
            v-model="draft.timerMaxCount"
            :min="0"
            :max="9999"
            :step="1"
            size="small"
            class="number-control"
            :placeholder="$t('manage.auto_backup_max_count_hint')"
          />
        </div>
        <div v-if="cloudGame" class="shared-retention">
          <div class="panel-heading">
            <h3>{{ $t('manage.shared_retention_limit') }}</h3>
            <span>{{ $t('manage.shared_retention_hint') }}</span>
          </div>
          <div class="retention-field">
            <el-switch v-model="sharedRetentionEnabled" />
            <el-input-number
              v-if="sharedRetentionEnabled"
              v-model="sharedRetentionLimit"
              :min="1"
              :max="1000"
              :step="1"
              size="small"
              class="number-control"
            />
          </div>
        </div>
      </section>

      <section class="settings-panel">
        <div class="panel-header">
          <div class="panel-heading">
            <h3>{{ $t('manage.process_monitor') }}</h3>
            <span>{{ $t('manage.process_monitor_summary') }}</span>
          </div>
          <el-switch v-model="draft.processEnabled" />
        </div>
        <div v-if="draft.processEnabled" class="panel-stack">
          <div class="field-row">
            <span class="field-label">{{ $t('manage.process_monitor_name') }}</span>
            <ProcessSelect
              v-model="draft.processName"
              :options="processOptions"
              :loading="loadingTargets"
              :placeholder="$t('manage.process_monitor_name_placeholder')"
              @refresh="refreshTargets"
            />
          </div>
          <div class="trigger-strip">
            <el-checkbox v-model="draft.onStart">{{
              $t('manage.process_monitor_on_start')
            }}</el-checkbox>
            <el-checkbox v-model="draft.onExit">{{
              $t('manage.process_monitor_on_exit')
            }}</el-checkbox>
            <el-checkbox v-model="draft.intervalEnabled">
              {{ $t('manage.process_monitor_interval') }}
            </el-checkbox>
          </div>
          <div v-if="draft.intervalEnabled" class="field-row compact">
            <span class="field-label">{{ $t('manage.process_monitor_interval_secs') }}</span>
            <el-input-number
              v-model="draft.processIntervalSecs"
              :min="1"
              :max="86400"
              :step="1"
              size="small"
              class="number-control"
            />
          </div>
        </div>
      </section>

      <p class="drawer-footnote">{{ $t('manage.auto_save_feedback_hint') }}</p>
    </div>
    <template #footer>
      <div class="drawer-footer">
        <el-button @click="visible = false">{{ $t('manage.cancel') }}</el-button>
        <el-button type="primary" :loading="saving" @click="saveDraft">
          {{ $t('manage.save_settings') }}
        </el-button>
      </div>
    </template>
  </el-drawer>
</template>

<style scoped>
.auto-save-drawer {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.settings-panel {
  border: 1px solid var(--el-border-color-light);
  border-radius: 10px;
  padding: 16px;
  background: var(--el-bg-color);
  transition: border-color 0.2s ease;
}

.settings-panel:hover {
  border-color: var(--el-border-color);
}

.panel-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
}

.panel-heading {
  min-width: 0;
}

.panel-heading h3 {
  margin: 0;
  font-size: 15px;
  font-weight: 650;
  line-height: 1.35;
}

.panel-heading span {
  display: block;
  margin-top: 4px;
  color: var(--el-text-color-secondary);
  font-size: 12px;
  line-height: 1.45;
}

.panel-grid {
  display: grid;
  grid-template-columns: 140px minmax(0, 1fr);
  align-items: center;
  gap: 10px 12px;
  margin-top: 14px;
}

.panel-stack {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-top: 14px;
}

.field-row {
  display: grid;
  grid-template-columns: 140px minmax(0, 1fr);
  align-items: center;
  gap: 12px;
}

.field-row.compact {
  max-width: 330px;
}

.field-label {
  color: var(--el-text-color-regular);
  font-size: 13px;
}

.field-control {
  width: 100%;
}

.number-control {
  width: 150px;
}

.trigger-strip {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px 12px;
}

.cloud-policy {
  display: grid;
  gap: 16px;
}

.shared-retention {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  margin-top: 16px;
}

.retention-field {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
}

.field-hint {
  grid-column: 1 / -1;
  color: var(--el-text-color-secondary);
  font-size: 12px;
  line-height: 1.45;
}

.drawer-footnote {
  margin: 0;
  color: var(--el-text-color-secondary);
  font-size: 12px;
  line-height: 1.5;
}

.drawer-footer {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
}
</style>
