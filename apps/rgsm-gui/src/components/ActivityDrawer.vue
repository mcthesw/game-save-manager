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
  CopyDocument,
  Check,
} from '@element-plus/icons-vue';
import { ref, computed, watch, onUnmounted } from 'vue';
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
const { activities, activityAddSignal, dismissActivity, dismissAll, notifyError } =
  useActivityCenter();

const expanded = ref(false);
let collapseTimer: ReturnType<typeof setTimeout> | null = null;
const collapseCountdown = ref(false);
const collapseDuration = ref(0);

// Two states only: ghost ball (collapsed) or full panel (expanded)
const isGhostTab = computed(() => !expanded.value);

const hasCloudSyncJobs = computed(() => jobs.value.length > 0);
const hasActivities = computed(() => activities.value.length > 0);

const activeActivityCount = computed(
  () => activities.value.filter((e) => e.status === 'pending' || e.status === 'running').length
);

const totalActiveCount = computed(() => activeJobs.value + activeActivityCount.value);
const hasActiveWork = computed(() => isSyncing.value || activeActivityCount.value > 0);
const hasErrors = computed(() => activities.value.some((e) => e.status === 'error'));
// All active work done — trigger auto-collapse timer
const isIdle = computed(() => !isSyncing.value && activeActivityCount.value === 0);

function clearCollapseTimer() {
  if (collapseTimer !== null) {
    clearTimeout(collapseTimer);
    collapseTimer = null;
  }
  collapseCountdown.value = false;
}

function scheduleCollapse() {
  clearCollapseTimer();
  const delay = hasErrors.value ? 20_000 : 3_000;
  collapseDuration.value = delay;
  collapseCountdown.value = true;
  collapseTimer = setTimeout(() => {
    expanded.value = false;
    collapseTimer = null;
    collapseCountdown.value = false;
  }, delay);
}

// Auto-collapse when all active work finishes
watch(isIdle, (idle) => {
  if (idle && expanded.value) {
    scheduleCollapse();
  } else {
    clearCollapseTimer();
  }
});

// Auto-expand on any new activity — watch the add-signal (not length) so eviction at MAX_HISTORY
// doesn't suppress the trigger.
watch(activityAddSignal, () => {
  clearCollapseTimer();
  expanded.value = true;
  if (isIdle.value) {
    scheduleCollapse();
  }
});

// Auto-expand when cloud sync starts
watch(isSyncing, (syncing) => {
  if (syncing) {
    clearCollapseTimer();
    expanded.value = true;
  }
});

// Re-evaluate collapse delay when errors are dismissed (may switch from 20s to 5s window)
watch(hasErrors, (nowHasErrors) => {
  if (!nowHasErrors && isIdle.value && collapseTimer !== null) {
    scheduleCollapse();
  }
});

onUnmounted(() => clearCollapseTimer());

function handleToggleExpanded() {
  clearCollapseTimer();
  expanded.value = !expanded.value;
}

// Copy error/warning entry text to clipboard
const copiedId = ref<string | null>(null);

async function copyActivity(entry: ActivityEntry) {
  const text = entry.description ? `${entry.title}\n${entry.description}` : entry.title;
  if (
    typeof navigator === 'undefined' ||
    !navigator.clipboard ||
    typeof navigator.clipboard.writeText !== 'function'
  ) {
    notifyError($t('activity_center.copy_failed'), $t('activity_center.copy_unavailable'));
    return;
  }

  try {
    await navigator.clipboard.writeText(text);
  } catch {
    notifyError($t('activity_center.copy_failed'), $t('activity_center.copy_failed_detail'));
    return;
  }

  copiedId.value = entry.id;
  setTimeout(() => {
    if (copiedId.value === entry.id) copiedId.value = null;
  }, 1500);
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
    <!-- Header pill (always visible, acts as toggle + header when expanded) -->
    <div class="activity-pill" @click="handleToggleExpanded">
      <el-icon
        v-if="isGhostTab"
        :size="18"
        class="activity-ghost-icon"
        :class="{ 'ghost-active': hasActiveWork }"
      >
        <Loading v-if="hasActiveWork" />
        <List v-else />
      </el-icon>
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
          <span v-if="!expanded && totalActiveCount > 0" class="activity-pill-count">
            {{ $t('activity_center.active_count', { count: totalActiveCount }) }}
          </span>
        </div>
        <div class="activity-pill-right">
          <el-button
            v-if="expanded && hasActivities"
            text
            size="small"
            class="pill-dismiss-btn"
            @click.stop="dismissAll"
          >
            {{ $t('activity_center.dismiss_all') }}
          </el-button>
          <el-button
            v-if="expanded && isSyncing"
            text
            size="small"
            class="pill-dismiss-btn"
            :loading="isCancelling"
            @click.stop="cancelSync"
          >
            {{ $t('cloud_sync.cancel') }}
          </el-button>
          <svg
            v-if="collapseCountdown"
            class="collapse-ring"
            :style="{ '--collapse-duration': collapseDuration + 'ms' }"
            width="18"
            height="18"
            viewBox="0 0 18 18"
          >
            <circle class="collapse-ring-track" cx="9" cy="9" r="7" />
            <circle class="collapse-ring-progress" cx="9" cy="9" r="7" />
          </svg>
          <el-icon :size="14" class="activity-pill-chevron">
            <component :is="expanded ? ArrowDown : ArrowUp" />
          </el-icon>
        </div>
      </template>
    </div>

    <!-- Expanded panel (content only, no header) -->
    <Transition name="activity-panel-slide">
      <div v-if="expanded" class="activity-panel">
        <div class="activity-panel-scroll">
          <!-- Cloud sync jobs -->
          <template v-if="hasCloudSyncJobs">
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
          </template>

          <!-- Divider -->
          <div v-if="hasCloudSyncJobs && hasActivities" class="activity-section-divider" />

          <!-- General activities -->
          <template v-if="hasActivities">
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
                <span class="activity-item-title" :title="entry.title"
                  >{{ entry.title
                  }}<span v-if="entry.count > 1" class="activity-item-count">
                    ×{{ entry.count }}</span
                  ></span
                >
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
                v-if="entry.status === 'error' || entry.status === 'warning'"
                class="activity-item-copy"
                :title="$t('activity_center.copy')"
                @click.stop="copyActivity(entry)"
              >
                <el-icon :size="12">
                  <Check v-if="copiedId === entry.id" />
                  <CopyDocument v-else />
                </el-icon>
              </button>
              <button
                v-if="canDismiss(entry)"
                class="activity-item-dismiss"
                :title="$t('activity_center.dismiss')"
                @click.stop="dismissActivity(entry.id)"
              >
                <el-icon :size="12"><Close /></el-icon>
              </button>
            </div>
          </template>

          <!-- Empty state -->
          <div v-if="!hasCloudSyncJobs && !hasActivities" class="activity-empty">
            {{ $t('activity_center.empty') }}
          </div>
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.activity-drawer {
  position: fixed;
  right: 20px;
  bottom: 20px;
  min-width: 380px;
  max-width: 560px;
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

.activity-drawer:not(.is-ghost-tab) .activity-pill:not(:has(.activity-pill-left)) {
  justify-content: flex-end;
  padding: 6px 14px;
}

.pill-dismiss-btn {
  font-size: 0.75rem !important;
  padding: 0 4px !important;
  height: auto !important;
  color: var(--el-text-color-secondary) !important;
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
  gap: 6px;
  flex-shrink: 0;
}

.collapse-ring {
  --circumference: 43.98;
}

.collapse-ring-track {
  fill: none;
  stroke: var(--el-border-color-lighter);
  stroke-width: 2;
}

.collapse-ring-progress {
  fill: none;
  stroke: var(--el-text-color-secondary);
  stroke-width: 2;
  stroke-linecap: round;
  stroke-dasharray: var(--circumference);
  stroke-dashoffset: 0;
  transform: rotate(-90deg);
  transform-origin: center;
  animation: collapse-ring-drain var(--collapse-duration) linear forwards;
}

@keyframes collapse-ring-drain {
  from {
    stroke-dashoffset: 0;
  }
  to {
    stroke-dashoffset: var(--circumference);
  }
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
  border-top: 1px solid color-mix(in oklab, var(--el-border-color-lighter) 80%, transparent);
}

.activity-panel-scroll {
  max-height: 132px;
  overflow-y: auto;
  padding: 6px 0;
  scrollbar-width: thin;
  scrollbar-color: color-mix(in oklab, var(--el-text-color-placeholder) 40%, transparent)
    transparent;
}

.activity-panel-scroll::-webkit-scrollbar {
  width: 4px;
}

.activity-panel-scroll::-webkit-scrollbar-track {
  background: transparent;
}

.activity-panel-scroll::-webkit-scrollbar-thumb {
  background: color-mix(in oklab, var(--el-text-color-placeholder) 40%, transparent);
  border-radius: 2px;
}

.activity-section-divider {
  height: 1px;
  background: color-mix(in oklab, var(--el-border-color-lighter) 80%, transparent);
  margin: 4px 14px;
}

.activity-empty {
  padding: 12px 14px;
  text-align: center;
  color: var(--el-text-color-secondary);
  font-size: 0.82rem;
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
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  color: var(--el-text-color-regular);
  white-space: normal;
  word-break: break-word;
}

.activity-item-count {
  color: var(--el-text-color-secondary);
  font-size: 0.85em;
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

.activity-item-copy {
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

.activity-item-copy:hover {
  color: var(--el-color-primary);
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
  max-height: 220px;
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
