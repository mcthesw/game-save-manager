<script setup lang="ts">
import { computed, ref } from 'vue';
import {
  CircleCloseFilled,
  CopyDocument,
  Loading,
  SuccessFilled,
  WarningFilled,
} from '@element-plus/icons-vue';
import { $t } from '../i18n';
import type {
  CloudBackendCheckItem,
  CloudBackendCheckReport,
  CloudBackendCheckStep,
} from '../bindings';

type CheckOutcome = CloudBackendCheckReport['outcome'];
type CheckItemStatus = CloudBackendCheckItem['status'];
type UiOutcome = CheckOutcome | 'unknown';
type UiCheckItem = {
  step: CloudBackendCheckStep;
  status: CheckItemStatus | 'pending';
  message: string | null;
};

const props = withDefaults(
  defineProps<{
    report: CloudBackendCheckReport | null;
    checking?: boolean;
    disabled?: boolean;
    error?: string | null;
  }>(),
  {
    checking: false,
    disabled: false,
    error: null,
  }
);

const emit = defineEmits<{
  (event: 'test'): void;
}>();

const copiedStep = ref<string | null>(null);

const STEPS: CloudBackendCheckStep[] = [
  'prepare_backend',
  'list_files',
  'write_file',
  'read_file',
  'verify_content',
  'delete_file',
];

const STEP_LABELS: Record<CloudBackendCheckStep, string> = {
  prepare_backend: 'sync_settings.check_result.steps.prepare_backend',
  list_files: 'sync_settings.check_result.steps.list_files',
  write_file: 'sync_settings.check_result.steps.write_file',
  read_file: 'sync_settings.check_result.steps.read_file',
  verify_content: 'sync_settings.check_result.steps.verify_content',
  delete_file: 'sync_settings.check_result.steps.delete_file',
};

const OUTCOME_META: Record<UiOutcome, { titleKey: string; descKey: string }> = {
  available: {
    titleKey: 'sync_settings.check_result.available_title',
    descKey: 'sync_settings.check_result.available_desc',
  },
  degraded: {
    titleKey: 'sync_settings.check_result.degraded_title',
    descKey: 'sync_settings.check_result.degraded_desc',
  },
  unavailable: {
    titleKey: 'sync_settings.check_result.unavailable_title',
    descKey: 'sync_settings.check_result.unavailable_desc',
  },
  unknown: {
    titleKey: 'sync_settings.check_result.unknown_title',
    descKey: 'sync_settings.check_result.unknown_desc',
  },
};

const currentOutcome = computed<UiOutcome>(() => {
  if (props.error) return 'unavailable';
  return props.report?.outcome ?? 'unknown';
});

const checkItems = computed<UiCheckItem[]>(() =>
  STEPS.map((step) => {
    const item = props.report?.items.find((candidate) => candidate.step === step);
    return item ?? { step, status: 'pending', message: null };
  })
);

const titleKey = computed(() =>
  props.checking
    ? 'sync_settings.check_result.checking_title'
    : OUTCOME_META[currentOutcome.value].titleKey
);

const descriptionKey = computed(() => {
  if (props.checking || currentOutcome.value === 'unknown') return null;
  return OUTCOME_META[currentOutcome.value].descKey;
});

function categorizeError(msg: string | null): string {
  if (!msg) return '';
  if (/timeout|timed out/i.test(msg))
    return $t('sync_settings.check_result.error_category.timeout');
  if (/401|403|unauthorized|forbidden/i.test(msg))
    return $t('sync_settings.check_result.error_category.auth');
  if (/404|not found/i.test(msg)) return $t('sync_settings.check_result.error_category.not_found');
  if (/network|connect|dns|resolve/i.test(msg))
    return $t('sync_settings.check_result.error_category.network');
  if (/virtual.host/i.test(msg)) return $t('sync_settings.check_result.error_category.addressing');
  return $t('sync_settings.check_result.error_category.general');
}

function truncate(str: string, max: number): string {
  return str.length > max ? str.slice(0, max) + '...' : str;
}

async function copyError(message: string, key: string) {
  await navigator.clipboard.writeText(message);
  copiedStep.value = key;
  setTimeout(() => {
    copiedStep.value = null;
  }, 2000);
}
</script>

<template>
  <section
    class="check-result-card"
    :class="[`is-${currentOutcome}`, { 'is-checking': checking }]"
    aria-live="polite"
  >
    <div class="check-summary">
      <span class="check-indicator" />
      <div class="check-summary-text">
        <span class="check-title">{{ $t(titleKey) }}</span>
        <span v-if="descriptionKey" class="check-desc">{{ $t(descriptionKey) }}</span>
      </div>
      <ElIcon v-if="checking" class="icon-spin"><Loading /></ElIcon>
    </div>

    <div class="check-steps">
      <div
        v-for="item in checkItems"
        :key="item.step"
        class="check-step"
        :class="`is-${item.status}`"
      >
        <span v-if="item.status === 'pending'" class="step-placeholder" />
        <ElIcon v-else class="step-icon">
          <SuccessFilled v-if="item.status === 'passed'" />
          <CircleCloseFilled v-else-if="item.status === 'failed'" />
          <WarningFilled v-else />
        </ElIcon>
        <span class="step-label">{{ $t(STEP_LABELS[item.step]) }}</span>
        <ElPopover v-if="item.message" trigger="click" placement="bottom-start" :width="360">
          <template #reference>
            <ElButton text size="small" class="step-error-btn">
              {{ $t('sync_settings.check_result.view_error') }}
            </ElButton>
          </template>
          <div class="error-popover">
            <p class="error-category">{{ categorizeError(item.message) }}</p>
            <pre class="error-raw">{{ truncate(item.message, 500) }}</pre>
            <ElButton size="small" @click="copyError(item.message, item.step)">
              <ElIcon class="copy-icon"><CopyDocument /></ElIcon>
              {{
                copiedStep === item.step
                  ? $t('sync_settings.check_result.copied')
                  : $t('sync_settings.check_result.copy_error')
              }}
            </ElButton>
          </div>
        </ElPopover>
      </div>
    </div>

    <div v-if="error" class="command-error">
      <span>{{ categorizeError(error) }}</span>
      <ElButton text size="small" @click="copyError(error, 'command')">
        {{
          copiedStep === 'command'
            ? $t('sync_settings.check_result.copied')
            : $t('sync_settings.check_result.copy_error')
        }}
      </ElButton>
    </div>

    <footer class="check-footer">
      <ElButton
        type="primary"
        :loading="checking"
        :disabled="disabled || checking"
        @click="emit('test')"
      >
        {{
          report || error
            ? $t('sync_settings.check_result.retest')
            : $t('sync_settings.test_button')
        }}
      </ElButton>
    </footer>
  </section>
</template>

<style scoped>
.check-result-card {
  display: flex;
  padding: 0 0 0 20px;
  flex-direction: column;
  border-left: 1px solid var(--el-border-color-lighter);
  background: transparent;
  box-sizing: border-box;
}

.check-summary {
  display: flex;
  align-items: flex-start;
  gap: 10px;
}

.check-indicator {
  width: 8px;
  height: 8px;
  margin-top: 5px;
  flex-shrink: 0;
  border-radius: 50%;
  background: var(--el-text-color-placeholder);
}

.is-available .check-indicator {
  background: var(--el-color-success);
}

.is-degraded .check-indicator {
  background: var(--el-color-warning);
}

.is-unavailable .check-indicator {
  background: var(--el-color-danger);
}

.is-checking .check-indicator {
  background: var(--el-color-primary);
}

.check-summary-text {
  min-width: 0;
  flex: 1;
}

.check-title {
  display: block;
  color: var(--el-text-color-primary);
  font-size: 0.92rem;
  font-weight: 600;
  line-height: 1.3;
}

.check-desc {
  display: block;
  margin-top: 4px;
  color: var(--el-text-color-secondary);
  font-size: 0.78rem;
  line-height: 1.45;
}

.icon-spin {
  flex-shrink: 0;
  color: var(--el-color-primary);
  animation: spin 1s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.check-steps {
  display: flex;
  margin-top: 18px;
  padding-top: 14px;
  flex-direction: column;
  gap: 10px;
  border-top: 1px solid var(--el-border-color-lighter);
}

.check-step {
  display: flex;
  min-height: 26px;
  align-items: center;
  gap: 8px;
}

.step-icon,
.step-placeholder {
  width: 14px;
  height: 14px;
  flex-shrink: 0;
}

.step-placeholder {
  border: 1.5px solid var(--el-border-color);
  border-radius: 50%;
  box-sizing: border-box;
}

.check-step.is-passed .step-icon {
  color: var(--el-color-success);
}

.check-step.is-warning .step-icon {
  color: var(--el-color-warning);
}

.check-step.is-failed .step-icon {
  color: var(--el-color-danger);
}

.step-label {
  min-width: 0;
  flex: 1;
  color: var(--el-text-color-regular);
  font-size: 0.82rem;
}

.check-step.is-pending .step-label {
  color: var(--el-text-color-placeholder);
}

.step-error-btn {
  flex-shrink: 0;
  font-size: 0.75rem !important;
}

.command-error {
  display: flex;
  margin-top: 14px;
  padding: 10px 12px;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  border-radius: 6px;
  background: var(--el-color-danger-light-8);
  color: var(--el-color-danger);
  font-size: 0.8rem;
}

.check-footer {
  margin-top: 18px;
}

.check-footer :deep(.el-button) {
  width: 100%;
}

.copy-icon {
  margin-right: 4px;
}

.error-popover .error-category {
  margin: 0 0 8px;
  color: var(--el-color-danger);
  font-size: 0.85rem;
  font-weight: 600;
}

.error-popover .error-raw {
  max-height: 200px;
  margin: 0 0 10px;
  padding: 8px;
  overflow: auto;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 4px;
  background: var(--el-fill-color-light);
  font-size: 0.75rem;
  line-height: 1.4;
  white-space: pre-wrap;
  word-break: break-all;
}

@media (prefers-reduced-motion: reduce) {
  .icon-spin {
    animation: none;
  }
}
</style>
