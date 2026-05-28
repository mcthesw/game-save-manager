<script setup lang="ts">
import { computed, type Component } from 'vue';
import {
  CircleCheckFilled,
  CircleCloseFilled,
  InfoFilled,
  WarningFilled,
} from '@element-plus/icons-vue';
import { $t } from '../i18n';
import type { CloudBackendCheckReport } from '../bindings';

type CheckOutcome = CloudBackendCheckReport['outcome'];
type CheckItem = CloudBackendCheckReport['items'][number];
type TagType = 'success' | 'warning' | 'danger' | 'info';

const props = defineProps<{
  report: CloudBackendCheckReport;
}>();

const OUTCOME_META: Record<
  CheckOutcome,
  { icon: Component; type: TagType; titleKey: string; descKey: string; badgeKey: string }
> = {
  available: {
    icon: CircleCheckFilled,
    type: 'success',
    titleKey: 'sync_settings.check_result.available_title',
    descKey: 'sync_settings.check_result.available_desc',
    badgeKey: 'sync_settings.check_result.available_badge',
  },
  degraded: {
    icon: WarningFilled,
    type: 'warning',
    titleKey: 'sync_settings.check_result.degraded_title',
    descKey: 'sync_settings.check_result.degraded_desc',
    badgeKey: 'sync_settings.check_result.degraded_badge',
  },
  unavailable: {
    icon: CircleCloseFilled,
    type: 'danger',
    titleKey: 'sync_settings.check_result.unavailable_title',
    descKey: 'sync_settings.check_result.unavailable_desc',
    badgeKey: 'sync_settings.check_result.unavailable_badge',
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

const outcomeMeta = computed(() => OUTCOME_META[props.report.outcome]);

function itemIcon(item: CheckItem) {
  if (item.status === 'passed') return CircleCheckFilled;
  if (item.status === 'warning') return WarningFilled;
  return CircleCloseFilled;
}

function itemTagType(item: CheckItem): TagType {
  if (item.status === 'passed') return 'success';
  if (item.status === 'warning') return 'warning';
  return 'danger';
}

function itemKindLabel(item: CheckItem) {
  return item.critical
    ? $t('sync_settings.check_result.required')
    : $t('sync_settings.check_result.optional');
}
</script>

<template>
  <section class="backend-check-result" :class="`is-${report.outcome}`" aria-live="polite">
    <header class="check-summary">
      <span class="summary-icon" :class="`is-${report.outcome}`">
        <ElIcon><component :is="outcomeMeta.icon" /></ElIcon>
      </span>
      <div class="summary-copy">
        <div class="summary-title">{{ $t(outcomeMeta.titleKey) }}</div>
        <div class="summary-desc">{{ $t(outcomeMeta.descKey) }}</div>
      </div>
      <ElTag size="small" :type="outcomeMeta.type" effect="light" round>
        {{ $t(outcomeMeta.badgeKey) }}
      </ElTag>
    </header>

    <div class="check-items">
      <ElTooltip
        v-for="item in report.items"
        :key="item.step"
        :content="item.message ?? ''"
        :disabled="!item.message"
        placement="top-start"
        :show-after="250"
      >
        <div class="check-item" :class="`is-${item.status}`">
          <span class="item-icon">
            <ElIcon><component :is="itemIcon(item)" /></ElIcon>
          </span>
          <span class="item-label">{{ $t(STEP_LABELS[item.step]) }}</span>
          <ElTag size="small" :type="itemTagType(item)" effect="plain" round>
            {{ itemKindLabel(item) }}
          </ElTag>
          <ElIcon v-if="item.message" class="detail-icon"><InfoFilled /></ElIcon>
        </div>
      </ElTooltip>
    </div>
  </section>
</template>

<style scoped>
.backend-check-result {
  width: min(100%, 520px);
  padding: 12px;
  border: 1px solid var(--el-border-color);
  border-radius: 8px;
  background: var(--el-bg-color);
  box-shadow: inset 3px 0 0 var(--el-border-color);
}

.backend-check-result.is-available {
  border-color: var(--el-color-success-light-5);
  box-shadow: inset 3px 0 0 var(--el-color-success);
}

.backend-check-result.is-degraded {
  border-color: var(--el-color-warning-light-5);
  box-shadow: inset 3px 0 0 var(--el-color-warning);
}

.backend-check-result.is-unavailable {
  border-color: var(--el-color-danger-light-5);
  box-shadow: inset 3px 0 0 var(--el-color-danger);
}

.check-summary {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 10px;
}

.summary-icon,
.item-icon {
  display: inline-grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  color: var(--el-text-color-primary);
}

.summary-icon.is-available {
  color: var(--el-color-success);
  background: var(--el-color-success-light-9);
}

.summary-icon.is-degraded {
  color: var(--el-color-warning);
  background: var(--el-color-warning-light-9);
}

.summary-icon.is-unavailable {
  color: var(--el-color-danger);
  background: var(--el-color-danger-light-9);
}

.summary-copy {
  min-width: 0;
}

.summary-title {
  color: var(--el-text-color-primary);
  font-size: 0.94rem;
  font-weight: 600;
  line-height: 1.3;
}

.summary-desc {
  margin-top: 2px;
  color: var(--el-text-color-secondary);
  font-size: 0.82rem;
  line-height: 1.4;
}

.check-items {
  display: grid;
  gap: 6px;
  margin-top: 12px;
}

.check-item {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto auto;
  align-items: center;
  gap: 8px;
  min-height: 32px;
  padding: 6px 8px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 6px;
  background: var(--el-fill-color-blank);
}

.check-item.is-passed .item-icon {
  color: var(--el-color-success);
}

.check-item.is-warning .item-icon {
  color: var(--el-color-warning);
}

.check-item.is-failed .item-icon {
  color: var(--el-color-danger);
}

.item-label {
  min-width: 0;
  overflow: hidden;
  color: var(--el-text-color-primary);
  font-size: 0.84rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.detail-icon {
  color: var(--el-text-color-secondary);
}

@media (max-width: 640px) {
  .check-summary,
  .check-item {
    grid-template-columns: auto minmax(0, 1fr);
  }

  .check-summary :deep(.el-tag),
  .check-item :deep(.el-tag),
  .detail-icon {
    justify-self: start;
    grid-column: 2;
  }
}
</style>
