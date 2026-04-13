<script setup lang="ts">
import {
  Loading,
  SuccessFilled,
  CircleCloseFilled,
  WarningFilled,
  InfoFilled,
  ArrowUp,
  ArrowDown,
  Close,
  List,
} from '@element-plus/icons-vue';
import { ref, computed, watch } from 'vue';
import { $t } from '../i18n';
import { useCloudSyncStatus } from '../composables/useCloudSyncStatus';
import type { CloudSyncJobStatus } from '../composables/useCloudSyncStatus';
import {
  useActivityCenter,
  type ActivityEntry,
  type ActivityStatus,
} from '../composables/useActivityCenter';
import { LAYER } from '../ui/layers';

const { activeJobs, isSyncing, isCancelling, jobs, cancelSync } = useCloudSyncStatus();
const { activities, activityAddSignal, dismissActivity, dismissAll } = useActivityCenter();

const expanded = ref(false);

// Two states only: ghost ball (collapsed) or full panel (expanded)
const isGhostTab = computed(() => !expanded.value);

const hasCloudSyncJobs = computed(() => jobs.value.length > 0);
const hasActivities = computed(() => activities.value.length > 0);

const activeActivityCount = computed(
  () => activities.value.filter((e) => e.status === 'pending' || e.status === 'running').length
);

const totalActiveCount = computed(() => activeJobs.value + activeActivityCount.value);
const hasActiveWork = computed(() => isSyncing.value || activeActivityCount.value > 0);

// Auto-expand on any new activity — watch the add-signal (not length) so eviction at MAX_HISTORY
// doesn't suppress the trigger. Also no isGlobalLoading guard — withLoading covers the UI, and
// we want the panel to be ready when loading completes.
watch(activityAddSignal, () => {
  expanded.value = true;
});

// Auto-expand when cloud sync starts
watch(isSyncing, (syncing) => {
  if (syncing) expanded.value = true;
});

function handleToggleExpanded() {
  expanded.value = !expanded.value;
}

// Cloud sync helpers (ported from CloudSyncIndicator)
function syncStatusIcon(status: CloudSyncJobStatus) {
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

function syncStatusClass(status: CloudSyncJobStatus) {
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

function syncStatusLabel(status: CloudSyncJobStatus) {
  return $t(`cloud_sync.status_${status.toLowerCase()}`);
}

// Activity helpers
function activityIcon(status: ActivityStatus) {
  switch (status) {
    case 'pending':
    case 'running':
      return Loading;
    case 'success':
      return SuccessFilled;
    case 'info':
      return InfoFilled;
    case 'warning':
      return WarningFilled;
    case 'error':
      return CircleCloseFilled;
    default:
      return null;
  }
}

function activityClass(status: ActivityStatus) {
  switch (status) {
    case 'pending':
    case 'running':
      return 'activity-running';
    case 'success':
      return 'activity-success';
    case 'info':
      return 'activity-info';
    case 'warning':
      return 'activity-warning';
    case 'error':
      return 'activity-error';
    default:
      return '';
  }
}

function activityStatusLabel(status: ActivityStatus) {
  return $t(`activity_center.status_${status}`);
}

function canDismiss(entry: ActivityEntry) {
  return entry.status !== 'pending' && entry.status !== 'running';
}
</script>

<template>
  <div
    class="activity-drawer"
    :class="{ 'is-ghost-tab': isGhostTab }"
    role="status"
    aria-live="polite"
    :style="{ zIndex: LAYER.activityDrawer }"
  >
    <!-- Expanded panel -->
    <Transition name="activity-panel-slide">
      <div v-if="expanded" class="activity-panel">
        <!-- Cloud sync section -->
        <div v-if="hasCloudSyncJobs" class="activity-section cloud-sync-section">
          <div class="activity-section-header">
            <span class="activity-section-title">{{ $t('cloud_sync.title') }}</span>
            <el-button
              v-if="isSyncing"
              text
              size="small"
              :loading="isCancelling"
              class="activity-section-action"
              @click.stop="cancelSync"
            >
              {{ $t('cloud_sync.cancel') }}
            </el-button>
          </div>
          <div class="activity-list">
            <div
              v-for="job in jobs"
              :key="job.id"
              class="activity-item"
              :class="syncStatusClass(job.status)"
            >
              <el-icon
                v-if="syncStatusIcon(job.status)"
                class="activity-item-icon"
                :class="{ 'icon-spin': job.status === 'Running' }"
                :size="14"
              >
                <component :is="syncStatusIcon(job.status)" />
              </el-icon>
              <span v-else class="activity-item-icon-placeholder" />
              <div class="activity-item-body">
                <span class="activity-item-title" :title="job.description">{{
                  job.description
                }}</span>
              </div>
              <span class="activity-item-badge">{{ syncStatusLabel(job.status) }}</span>
            </div>
          </div>
        </div>

        <!-- Divider between sections -->
        <div v-if="hasCloudSyncJobs && hasActivities" class="activity-section-divider" />

        <!-- General activities section -->
        <div v-if="hasActivities" class="activity-section">
          <div class="activity-section-header">
            <span class="activity-section-title">{{ $t('activity_center.title') }}</span>
            <el-button text size="small" class="activity-section-action" @click.stop="dismissAll">
              {{ $t('activity_center.dismiss_all') }}
            </el-button>
          </div>
          <div class="activity-list">
            <div v-if="activities.length === 0" class="activity-empty">
              {{ $t('activity_center.empty') }}
            </div>
            <div
              v-for="entry in [...activities].reverse()"
              :key="entry.id"
              class="activity-item"
              :class="activityClass(entry.status)"
            >
              <el-icon
                v-if="activityIcon(entry.status)"
                class="activity-item-icon"
                :class="{ 'icon-spin': entry.status === 'running' || entry.status === 'pending' }"
                :size="14"
              >
                <component :is="activityIcon(entry.status)" />
              </el-icon>
              <span v-else class="activity-item-icon-placeholder" />
              <div class="activity-item-body">
                <span class="activity-item-title" :title="entry.title">{{ entry.title }}</span>
                <span
                  v-if="entry.description"
                  class="activity-item-desc"
                  :title="entry.description"
                >
                  {{ entry.description }}
                </span>
              </div>
              <span class="activity-item-badge">{{ activityStatusLabel(entry.status) }}</span>
              <button
                v-if="canDismiss(entry)"
                class="activity-item-dismiss"
                :title="$t('activity_center.dismiss')"
                @click.stop="dismissActivity(entry.id)"
              >
                <el-icon :size="12"><Close /></el-icon>
              </button>
            </div>
          </div>
        </div>

        <!-- Empty state when panel open but nothing to show -->
        <div v-if="!hasCloudSyncJobs && !hasActivities" class="activity-empty activity-empty-panel">
          {{ $t('activity_center.empty') }}
        </div>
      </div>
    </Transition>

    <!-- Collapsed pill bar -->
    <div class="activity-pill" @click="handleToggleExpanded">
      <!-- Ghost ball: collapsed state, always shows — spinner when active work, list icon when idle -->
      <el-icon
        v-if="isGhostTab"
        :size="18"
        class="activity-ghost-icon"
        :class="{ 'ghost-active': hasActiveWork }"
      >
        <Loading v-if="hasActiveWork" />
        <List v-else />
      </el-icon>
      <!-- Normal pill: full info bar when has content or is expanded -->
      <template v-else>
        <div class="activity-pill-left">
          <el-icon
            v-if="isSyncing || activeActivityCount > 0"
            class="activity-pill-spinner"
            :size="16"
          >
            <Loading />
          </el-icon>
          <span class="activity-pill-title">{{ $t('activity_center.title') }}</span>
          <span v-if="totalActiveCount > 0" class="activity-pill-count">
            {{ $t('activity_center.active_count', { count: totalActiveCount }) }}
          </span>
        </div>
        <div class="activity-pill-right">
          <el-icon :size="14" class="activity-pill-chevron">
            <component :is="expanded ? ArrowDown : ArrowUp" />
          </el-icon>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.activity-drawer {
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
  transition:
    min-width 0.28s cubic-bezier(0.4, 0, 0.2, 1),
    max-width 0.28s cubic-bezier(0.4, 0, 0.2, 1),
    border-radius 0.28s cubic-bezier(0.4, 0, 0.2, 1),
    box-shadow 0.2s ease,
    background-color 0.2s ease;
}

/* Ghost tab: a small floating circle anchored bottom-right */
.activity-drawer.is-ghost-tab {
  min-width: 40px;
  max-width: 40px;
  border-radius: 50%;
  background: color-mix(in oklab, var(--el-bg-color-overlay) 72%, transparent);
  box-shadow:
    0 4px 14px rgba(0, 0, 0, 0.14),
    inset 0 0 0 1px color-mix(in oklab, var(--el-border-color) 50%, transparent);
  cursor: pointer;
}

.activity-drawer.is-ghost-tab:hover {
  background: color-mix(in oklab, var(--el-bg-color-overlay) 95%, transparent);
  box-shadow:
    0 6px 20px rgba(0, 0, 0, 0.2),
    inset 0 0 0 1px color-mix(in oklab, var(--el-color-primary) 45%, transparent);
}

.activity-ghost-icon {
  color: var(--el-text-color-secondary);
  transition: color 0.15s ease;
}

.activity-ghost-icon.ghost-active {
  color: var(--el-color-primary);
  animation: activity-spin 1s linear infinite;
}

.activity-drawer.is-ghost-tab:hover .activity-ghost-icon {
  color: var(--el-color-primary);
}

/* Pill bar */
.activity-pill {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  cursor: pointer;
  user-select: none;
  gap: 10px;
}

/* Ghost tab pill: center the icon in a 40×40 square */
.activity-drawer.is-ghost-tab .activity-pill {
  padding: 0;
  width: 40px;
  height: 40px;
  justify-content: center;
  align-items: center;
}

.activity-pill-left {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.activity-pill-title {
  font-size: 0.88rem;
  font-weight: 600;
  color: var(--el-text-color-primary);
  white-space: nowrap;
}

.activity-pill-count {
  font-size: 0.78rem;
  color: var(--el-text-color-secondary);
  white-space: nowrap;
}

.activity-pill-right {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}

.activity-pill-chevron {
  color: var(--el-text-color-secondary);
  transition: transform 0.2s ease;
}

.activity-pill-spinner {
  color: var(--el-color-primary);
  animation: activity-spin 1s linear infinite;
  flex-shrink: 0;
}

/* Panel */
.activity-panel {
  border-bottom: 1px solid color-mix(in oklab, var(--el-border-color-lighter) 80%, transparent);
  max-height: 480px;
  overflow-y: auto;
}

.activity-section {
  padding: 8px 0;
}

.activity-section-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 2px 14px 4px;
}

.activity-section-title {
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--el-text-color-secondary);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.activity-section-action {
  font-size: 0.75rem;
  padding: 0 4px;
  height: auto;
  color: var(--el-text-color-secondary);
}

.activity-section-divider {
  height: 1px;
  background: color-mix(in oklab, var(--el-border-color-lighter) 80%, transparent);
  margin: 0 14px;
}

.activity-list {
  max-height: 220px;
  overflow-y: auto;
}

.activity-empty {
  padding: 8px 14px;
  text-align: center;
  color: var(--el-text-color-secondary);
  font-size: 0.82rem;
}

.activity-empty-panel {
  padding: 16px;
}

/* Activity rows */
.activity-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 14px;
  font-size: 0.8rem;
  line-height: 1.3;
}

.activity-item-icon {
  flex-shrink: 0;
}

.activity-item-icon-placeholder {
  width: 14px;
  height: 14px;
  flex-shrink: 0;
}

.activity-item-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.activity-item-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--el-text-color-regular);
}

.activity-item-desc {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.72rem;
  color: var(--el-text-color-secondary);
}

.activity-item-badge {
  flex-shrink: 0;
  font-size: 0.72rem;
  padding: 1px 6px;
  border-radius: 4px;
  white-space: nowrap;
}

.activity-item-dismiss {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border: none;
  background: transparent;
  cursor: pointer;
  border-radius: 3px;
  color: var(--el-text-color-placeholder);
  padding: 0;
  transition:
    color 0.15s ease,
    background-color 0.15s ease;
}

.activity-item-dismiss:hover {
  color: var(--el-text-color-secondary);
  background: color-mix(in oklab, var(--el-fill-color) 80%, transparent);
}

/* Status colours for cloud sync jobs */
.job-running .activity-item-icon {
  color: var(--el-color-primary);
}
.job-running .activity-item-badge {
  background: color-mix(in oklab, var(--el-color-primary) 14%, transparent);
  color: var(--el-color-primary);
}
.job-queued .activity-item-badge {
  background: color-mix(in oklab, var(--el-text-color-secondary) 12%, transparent);
  color: var(--el-text-color-secondary);
}
.job-completed .activity-item-icon {
  color: var(--el-color-success);
}
.job-completed .activity-item-badge {
  background: color-mix(in oklab, var(--el-color-success) 14%, transparent);
  color: var(--el-color-success);
}
.job-failed .activity-item-icon {
  color: var(--el-color-danger);
}
.job-failed .activity-item-badge {
  background: color-mix(in oklab, var(--el-color-danger) 14%, transparent);
  color: var(--el-color-danger);
}
.job-cancelled .activity-item-icon {
  color: var(--el-text-color-secondary);
}
.job-cancelled .activity-item-badge {
  background: color-mix(in oklab, var(--el-text-color-secondary) 12%, transparent);
  color: var(--el-text-color-secondary);
}

/* Status colours for activity entries */
.activity-running .activity-item-icon {
  color: var(--el-color-primary);
}
.activity-running .activity-item-badge {
  background: color-mix(in oklab, var(--el-color-primary) 14%, transparent);
  color: var(--el-color-primary);
}
.activity-success .activity-item-icon {
  color: var(--el-color-success);
}
.activity-success .activity-item-badge {
  background: color-mix(in oklab, var(--el-color-success) 14%, transparent);
  color: var(--el-color-success);
}
.activity-info .activity-item-icon {
  color: var(--el-color-info);
}
.activity-info .activity-item-badge {
  background: color-mix(in oklab, var(--el-color-info) 14%, transparent);
  color: var(--el-color-info);
}
.activity-warning .activity-item-icon {
  color: var(--el-color-warning);
}
.activity-warning .activity-item-badge {
  background: color-mix(in oklab, var(--el-color-warning) 14%, transparent);
  color: var(--el-color-warning);
}
.activity-error .activity-item-icon {
  color: var(--el-color-danger);
}
.activity-error .activity-item-badge {
  background: color-mix(in oklab, var(--el-color-danger) 14%, transparent);
  color: var(--el-color-danger);
}

/* Spin animation */
.icon-spin {
  animation: activity-spin 1s linear infinite;
}

/* Transitions */
.activity-panel-slide-enter-active,
.activity-panel-slide-leave-active {
  transition:
    max-height 0.25s ease,
    opacity 0.2s ease;
  overflow: hidden;
}
.activity-panel-slide-enter-from,
.activity-panel-slide-leave-to {
  max-height: 0;
  opacity: 0;
}
.activity-panel-slide-enter-to,
.activity-panel-slide-leave-from {
  max-height: 500px;
  opacity: 1;
}

@keyframes activity-spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>
