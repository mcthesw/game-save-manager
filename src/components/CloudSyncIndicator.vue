<script setup lang="ts">
import {
  Loading,
  SuccessFilled,
  CircleCloseFilled,
  WarningFilled,
  ArrowUp,
  ArrowDown,
} from '@element-plus/icons-vue';

import { ref, computed } from 'vue';
import { $t } from '../i18n';
import { useCloudSyncStatus } from '../composables/useCloudSyncStatus';
import type { CloudSyncJobStatus } from '../composables/useCloudSyncStatus';
import { LAYER } from '../ui/layers';

const { activeJobs, isSyncing, isCancelling, jobs, cancelSync } = useCloudSyncStatus();

const expanded = ref(false);

const hasJobs = computed(() => jobs.value.length > 0);
const isVisible = computed(() => isSyncing.value || (expanded.value && hasJobs.value));

function toggleExpanded() {
  expanded.value = !expanded.value;
}

function statusIcon(status: CloudSyncJobStatus) {
  switch (status) {
    case 'Running':
      return Loading;
    case 'Completed':
      return SuccessFilled;
    case 'Failed':
      return WarningFilled;
    case 'Cancelled':
      return CircleCloseFilled;
    default:
      return null;
  }
}

function statusClass(status: CloudSyncJobStatus) {
  switch (status) {
    case 'Running':
      return 'job-running';
    case 'Completed':
      return 'job-completed';
    case 'Failed':
      return 'job-failed';
    case 'Cancelled':
      return 'job-cancelled';
    case 'Queued':
      return 'job-queued';
    default:
      return '';
  }
}

function statusLabel(status: CloudSyncJobStatus) {
  const key = `cloud_sync.status_${status.toLowerCase()}`;
  return $t(key);
}
</script>

<template>
  <Transition name="cloud-sync-indicator-fade">
    <div
      v-if="isVisible"
      class="cloud-sync-indicator"
      role="status"
      aria-live="polite"
      :style="{ zIndex: LAYER.cloudSyncIndicator }"
    >
      <!-- Expanded job list panel -->
      <Transition name="cloud-sync-panel-slide">
        <div v-if="expanded" class="cloud-sync-panel">
          <div class="cloud-sync-job-list">
            <div v-if="!hasJobs" class="cloud-sync-empty">
              {{ $t('cloud_sync.no_recent_jobs') }}
            </div>
            <div
              v-for="job in jobs"
              :key="job.id"
              class="cloud-sync-job-item"
              :class="statusClass(job.status)"
            >
              <el-icon
                v-if="statusIcon(job.status)"
                class="job-icon"
                :class="{ 'job-spin': job.status === 'Running' }"
                :size="14"
              >
                <component :is="statusIcon(job.status)" />
              </el-icon>
              <span v-else class="job-icon-placeholder" />
              <span class="job-desc" :title="job.description">{{ job.description }}</span>
              <span class="job-status-tag">{{ statusLabel(job.status) }}</span>
            </div>
          </div>
        </div>
      </Transition>

      <!-- Collapsed pill bar -->
      <div class="cloud-sync-pill" @click="toggleExpanded">
        <div class="cloud-sync-pill-left">
          <el-icon v-if="isSyncing" class="cloud-sync-spinner" :size="16">
            <Loading />
          </el-icon>
          <span class="cloud-sync-pill-title">{{ $t('cloud_sync.title') }}</span>
          <span v-if="isSyncing" class="cloud-sync-pill-count">
            {{ $t('cloud_sync.active_count', { count: activeJobs }) }}
          </span>
        </div>
        <div class="cloud-sync-pill-right">
          <el-button
            v-if="isSyncing"
            text
            size="small"
            :loading="isCancelling"
            @click.stop="cancelSync"
          >
            {{ $t('cloud_sync.cancel') }}
          </el-button>
          <el-icon :size="14" class="cloud-sync-expand-icon">
            <component :is="expanded ? ArrowDown : ArrowUp" />
          </el-icon>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.cloud-sync-indicator {
  position: fixed;
  right: 20px;
  bottom: 20px;
  min-width: 340px;
  max-width: 460px;
  display: flex;
  flex-direction: column;
  border-radius: 12px;
  background: color-mix(in oklab, var(--el-bg-color-overlay) 94%, transparent);
  box-shadow:
    0 12px 34px rgba(0, 0, 0, 0.22),
    inset 0 0 0 1px color-mix(in oklab, var(--el-border-color) 68%, transparent);
  backdrop-filter: blur(6px);
  overflow: hidden;
}

/* Collapsed pill bar */
.cloud-sync-pill {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  cursor: pointer;
  user-select: none;
  gap: 10px;
}

.cloud-sync-pill-left {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.cloud-sync-pill-title {
  font-size: 0.88rem;
  font-weight: 600;
  color: var(--el-text-color-primary);
  white-space: nowrap;
}

.cloud-sync-pill-count {
  font-size: 0.78rem;
  color: var(--el-text-color-secondary);
  white-space: nowrap;
}

.cloud-sync-pill-right {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}

.cloud-sync-expand-icon {
  color: var(--el-text-color-secondary);
  transition: transform 0.2s ease;
}

.cloud-sync-spinner {
  color: var(--el-color-primary);
  animation: cloud-sync-spin 1s linear infinite;
  flex-shrink: 0;
}

/* Expanded panel */
.cloud-sync-panel {
  border-bottom: 1px solid color-mix(in oklab, var(--el-border-color-lighter) 80%, transparent);
}

.cloud-sync-job-list {
  max-height: 240px;
  overflow-y: auto;
  padding: 8px 0;
}

.cloud-sync-empty {
  padding: 16px;
  text-align: center;
  color: var(--el-text-color-secondary);
  font-size: 0.82rem;
}

.cloud-sync-job-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 14px;
  font-size: 0.8rem;
  line-height: 1.3;
}

.job-icon {
  flex-shrink: 0;
}

.job-icon-placeholder {
  width: 14px;
  height: 14px;
  flex-shrink: 0;
}

.job-desc {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--el-text-color-regular);
}

.job-status-tag {
  flex-shrink: 0;
  font-size: 0.72rem;
  padding: 1px 6px;
  border-radius: 4px;
  white-space: nowrap;
}

/* Status colours */
.job-running .job-icon {
  color: var(--el-color-primary);
}
.job-running .job-status-tag {
  background: color-mix(in oklab, var(--el-color-primary) 14%, transparent);
  color: var(--el-color-primary);
}

.job-queued .job-status-tag {
  background: color-mix(in oklab, var(--el-text-color-secondary) 12%, transparent);
  color: var(--el-text-color-secondary);
}

.job-completed .job-icon {
  color: var(--el-color-success);
}
.job-completed .job-status-tag {
  background: color-mix(in oklab, var(--el-color-success) 14%, transparent);
  color: var(--el-color-success);
}

.job-failed .job-icon {
  color: var(--el-color-danger);
}
.job-failed .job-status-tag {
  background: color-mix(in oklab, var(--el-color-danger) 14%, transparent);
  color: var(--el-color-danger);
}

.job-cancelled .job-icon {
  color: var(--el-text-color-secondary);
}
.job-cancelled .job-status-tag {
  background: color-mix(in oklab, var(--el-text-color-secondary) 12%, transparent);
  color: var(--el-text-color-secondary);
}

.job-spin {
  animation: cloud-sync-spin 1s linear infinite;
}

/* Transitions */
.cloud-sync-indicator-fade-enter-active,
.cloud-sync-indicator-fade-leave-active {
  transition:
    opacity 0.2s ease,
    transform 0.2s ease;
}
.cloud-sync-indicator-fade-enter-from,
.cloud-sync-indicator-fade-leave-to {
  opacity: 0;
  transform: translateY(6px);
}

.cloud-sync-panel-slide-enter-active,
.cloud-sync-panel-slide-leave-active {
  transition:
    max-height 0.25s ease,
    opacity 0.2s ease;
  overflow: hidden;
}
.cloud-sync-panel-slide-enter-from,
.cloud-sync-panel-slide-leave-to {
  max-height: 0;
  opacity: 0;
}
.cloud-sync-panel-slide-enter-to,
.cloud-sync-panel-slide-leave-from {
  max-height: 300px;
  opacity: 1;
}

@keyframes cloud-sync-spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>
