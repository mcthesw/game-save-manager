<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import {
  ArrowDown,
  SuccessFilled,
  CircleCloseFilled,
  WarningFilled,
  Loading,
  CopyDocument,
} from '@element-plus/icons-vue';
import { $t } from '../i18n';
import type { CloudBackendCheckReport } from '../bindings';

type CheckOutcome = CloudBackendCheckReport['outcome'];
type CheckItem = CloudBackendCheckReport['items'][number];
type UiOutcome = CheckOutcome | 'unknown';

const props = withDefaults(
  defineProps<{
    report: CloudBackendCheckReport | null;
    checking?: boolean;
  }>(),
  { checking: false }
);

const detailsExpanded = ref(false);
const copiedStep = ref<string | null>(null);

const visible = computed(() => props.checking || props.report !== null);
const currentOutcome = computed<UiOutcome>(() => props.report?.outcome ?? 'unknown');
const checkItems = computed(() => props.report?.items ?? []);

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

const STEP_LABELS: Record<CheckItem['step'], string> = {
  prepare_backend: 'sync_settings.check_result.steps.prepare_backend',
  list_files: 'sync_settings.check_result.steps.list_files',
  write_file: 'sync_settings.check_result.steps.write_file',
  read_file: 'sync_settings.check_result.steps.read_file',
  verify_content: 'sync_settings.check_result.steps.verify_content',
  delete_file: 'sync_settings.check_result.steps.delete_file',
};

watch(
  () => props.report,
  (report) => {
    if (!report) {
      detailsExpanded.value = false;
      return;
    }
    detailsExpanded.value = report.outcome !== 'available';
  }
);

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

async function copyError(item: CheckItem) {
  if (!item.message) return;
  await navigator.clipboard.writeText(item.message);
  copiedStep.value = item.step;
  setTimeout(() => {
    copiedStep.value = null;
  }, 2000);
}
</script>

<template>
  <Transition name="check-fade">
    <section
      v-if="visible"
      class="check-result-card"
      :class="`is-${currentOutcome}`"
      aria-live="polite"
    >
      <div v-if="checking" class="check-loading">
        <ElIcon class="icon-spin"><Loading /></ElIcon>
        <span>{{ $t('sync_settings.check_result.checking_desc') }}</span>
      </div>

      <template v-else>
        <div class="check-summary" @click="detailsExpanded = !detailsExpanded">
          <span class="check-indicator" />
          <div class="check-summary-text">
            <span class="check-title">{{ $t(OUTCOME_META[currentOutcome].titleKey) }}</span>
            <span class="check-desc">{{ $t(OUTCOME_META[currentOutcome].descKey) }}</span>
          </div>
          <ElIcon
            v-if="checkItems.length"
            class="check-toggle"
            :class="{ 'is-expanded': detailsExpanded }"
          >
            <ArrowDown />
          </ElIcon>
        </div>

        <ElCollapseTransition>
          <div v-if="detailsExpanded && checkItems.length" class="check-steps">
            <div
              v-for="item in checkItems"
              :key="item.step"
              class="check-step"
              :class="`is-${item.status}`"
            >
              <ElIcon class="step-icon">
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
                  <ElButton size="small" @click="copyError(item)">
                    <ElIcon style="margin-right: 4px"><CopyDocument /></ElIcon>
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
        </ElCollapseTransition>
      </template>
    </section>
  </Transition>
</template>

<style scoped>
.check-fade-enter-active,
.check-fade-leave-active {
  transition:
    opacity 0.25s ease,
    transform 0.25s ease;
}

.check-fade-enter-from,
.check-fade-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

.check-result-card {
  margin-top: 16px;
  padding: 12px 16px;
  border-radius: 8px;
  border: 1px solid var(--el-border-color-lighter);
  background: var(--el-fill-color-lighter);
  transition:
    border-color 0.2s,
    background-color 0.2s;
}

.check-result-card.is-available {
  border-color: var(--el-color-success-light-5);
  background: var(--el-color-success-light-9);
}

.check-result-card.is-degraded {
  border-color: var(--el-color-warning-light-5);
  background: var(--el-color-warning-light-9);
}

.check-result-card.is-unavailable {
  border-color: var(--el-color-danger-light-5);
  background: var(--el-color-danger-light-9);
}

.check-loading {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--el-text-color-secondary);
  font-size: 0.85rem;
}

.icon-spin {
  animation: spin 1s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.check-summary {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  cursor: pointer;
  user-select: none;
}

.check-indicator {
  width: 8px;
  height: 8px;
  margin-top: 5px;
  border-radius: 50%;
  flex-shrink: 0;
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
.is-unknown .check-indicator {
  background: var(--el-text-color-placeholder);
}

.check-summary-text {
  flex: 1;
  min-width: 0;
}

.check-title {
  display: block;
  font-size: 0.88rem;
  font-weight: 600;
  line-height: 1.3;
  color: var(--el-text-color-primary);
}

.check-desc {
  display: block;
  margin-top: 2px;
  font-size: 0.78rem;
  line-height: 1.4;
  color: var(--el-text-color-secondary);
}

.check-toggle {
  flex-shrink: 0;
  margin-top: 3px;
  transition: transform 0.2s;
  color: var(--el-text-color-secondary);
}

.check-toggle.is-expanded {
  transform: rotate(180deg);
}

.check-steps {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 12px;
  padding-top: 10px;
  border-top: 1px solid var(--el-border-color-lighter);
}

.check-step {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 26px;
}

.step-icon {
  font-size: 14px;
  flex-shrink: 0;
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
  flex: 1;
  font-size: 0.82rem;
  color: var(--el-text-color-regular);
}

.step-error-btn {
  flex-shrink: 0;
  font-size: 0.75rem !important;
}

.error-popover .error-category {
  margin: 0 0 8px;
  font-weight: 600;
  font-size: 0.85rem;
  color: var(--el-color-danger);
}

.error-popover .error-raw {
  margin: 0 0 10px;
  padding: 8px;
  max-height: 200px;
  overflow: auto;
  font-size: 0.75rem;
  line-height: 1.4;
  white-space: pre-wrap;
  word-break: break-all;
  background: var(--el-fill-color-light);
  border-radius: 4px;
  border: 1px solid var(--el-border-color-lighter);
}
</style>
