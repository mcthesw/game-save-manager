import { ref, computed } from 'vue';

export type ActivityStatus = 'pending' | 'running' | 'info' | 'success' | 'warning' | 'error';

export interface ActivityEntry {
  id: string;
  title: string;
  description?: string;
  status: ActivityStatus;
  createdAt: number;
  updatedAt: number;
  autoDismissMs: number;
}

// No auto-dismiss — entries persist until the user dismisses them manually
const DEFAULT_AUTO_DISMISS: Record<ActivityStatus, number> = {
  pending: 0,
  running: 0,
  info: 0,
  success: 0,
  warning: 0,
  error: 0,
};

const MAX_HISTORY = 50;

const activities = ref<ActivityEntry[]>([]);
// Signal that increments on every add — reliable trigger regardless of eviction at MAX_HISTORY
const activityAddSignal = ref(0);

const dismissTimers = new Map<string, ReturnType<typeof setTimeout>>();

function generateId() {
  return Date.now().toString(36) + Math.random().toString(36).substring(2, 6);
}

function isActive(status: ActivityStatus) {
  return status === 'pending' || status === 'running';
}

function isTerminal(status: ActivityStatus) {
  return !isActive(status);
}

function scheduleAutoDismiss(id: string, ms: number) {
  clearDismissTimer(id);
  dismissTimers.set(
    id,
    setTimeout(() => {
      dismissActivity(id);
    }, ms)
  );
}

function clearDismissTimer(id: string) {
  const t = dismissTimers.get(id);
  if (t !== undefined) {
    clearTimeout(t);
    dismissTimers.delete(id);
  }
}

function applyTimerPolicy(entry: ActivityEntry) {
  if ((entry.status === 'success' || entry.status === 'info') && entry.autoDismissMs > 0) {
    scheduleAutoDismiss(entry.id, entry.autoDismissMs);
  } else {
    clearDismissTimer(entry.id);
  }
}

function evict() {
  if (activities.value.length <= MAX_HISTORY) return;
  // Prefer evicting oldest auto-dismissible terminal entries (success/info)
  const autoDismissible = activities.value
    .filter((e) => isTerminal(e.status) && e.autoDismissMs > 0)
    .sort((a, b) => a.createdAt - b.createdAt);
  if (autoDismissible[0]) {
    clearDismissTimer(autoDismissible[0].id);
    activities.value = activities.value.filter((e) => e.id !== autoDismissible[0]!.id);
    return;
  }
  // Fallback: evict oldest non-active, non-error, non-warning entry
  const safeTerminal = activities.value
    .filter((e) => isTerminal(e.status) && e.status !== 'error' && e.status !== 'warning')
    .sort((a, b) => a.createdAt - b.createdAt);
  if (safeTerminal[0]) {
    clearDismissTimer(safeTerminal[0].id);
    activities.value = activities.value.filter((e) => e.id !== safeTerminal[0]!.id);
    return;
  }
  // Last resort: evict oldest entry
  const oldest = [...activities.value].sort((a, b) => a.createdAt - b.createdAt)[0];
  if (oldest) {
    clearDismissTimer(oldest.id);
    activities.value = activities.value.filter((e) => e.id !== oldest.id);
  }
}

export function addActivity(opts: {
  title: string;
  description?: string;
  status?: ActivityStatus;
  autoDismissMs?: number;
}): string {
  const status = opts.status ?? 'pending';
  const autoDismissMs =
    opts.autoDismissMs !== undefined ? opts.autoDismissMs : DEFAULT_AUTO_DISMISS[status];
  const now = Date.now();
  const id = generateId();
  const entry: ActivityEntry = {
    id,
    title: opts.title,
    description: opts.description,
    status,
    createdAt: now,
    updatedAt: now,
    autoDismissMs,
  };
  activities.value.push(entry);
  activityAddSignal.value++;
  evict();
  applyTimerPolicy(entry);
  return id;
}

export function updateActivity(
  id: string,
  patch: Partial<Pick<ActivityEntry, 'title' | 'description' | 'status' | 'autoDismissMs'>>
) {
  const idx = activities.value.findIndex((e) => e.id === id);
  if (idx === -1) return;
  const entry = activities.value[idx]!;
  const newStatus = patch.status ?? entry.status;
  const newAutoDismissMs =
    patch.autoDismissMs !== undefined
      ? patch.autoDismissMs
      : patch.status !== undefined
        ? DEFAULT_AUTO_DISMISS[newStatus]
        : entry.autoDismissMs;
  const updated: ActivityEntry = {
    ...entry,
    ...patch,
    autoDismissMs: newAutoDismissMs,
    updatedAt: Date.now(),
  };
  activities.value[idx] = updated;
  applyTimerPolicy(updated);
}

export function dismissActivity(id: string) {
  clearDismissTimer(id);
  activities.value = activities.value.filter((e) => e.id !== id);
}

export function dismissAll() {
  for (const t of dismissTimers.values()) {
    clearTimeout(t);
  }
  dismissTimers.clear();
  // Keep running/pending entries; dismiss terminal ones
  activities.value = activities.value.filter((e) => isActive(e.status));
}

export function notifySuccess(
  title: string,
  description?: string,
  opts?: { autoDismissMs?: number }
) {
  return addActivity({ title, description, status: 'success', ...opts });
}

export function notifyError(
  title: string,
  description?: string,
  opts?: { autoDismissMs?: number }
) {
  return addActivity({ title, description, status: 'error', ...opts });
}

export function notifyWarning(
  title: string,
  description?: string,
  opts?: { autoDismissMs?: number }
) {
  return addActivity({ title, description, status: 'warning', ...opts });
}

export function notifyInfo(title: string, description?: string, opts?: { autoDismissMs?: number }) {
  return addActivity({ title, description, status: 'info', ...opts });
}

export function useActivityCenter() {
  const hasActiveActivities = computed(() => activities.value.some((e) => isActive(e.status)));

  return {
    activities,
    activityAddSignal,
    hasActiveActivities,
    addActivity,
    updateActivity,
    dismissActivity,
    dismissAll,
    notifySuccess,
    notifyError,
    notifyWarning,
    notifyInfo,
  };
}
