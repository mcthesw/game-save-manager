import { ref } from 'vue';

/**
 * Transient foreground notifications. The activity drawer stays the history
 * log; toasts carry the "something just happened" signal and auto-dismiss.
 * Error toasts are sticky — failures must be acknowledged, never missed.
 */
export type ToastTone = 'success' | 'info' | 'warning' | 'error';

export interface ToastItem {
  id: string;
  tone: ToastTone;
  title: string;
  description?: string;
}

const AUTO_DISMISS_MS: Record<ToastTone, number> = {
  success: 3500,
  info: 3500,
  warning: 6000,
  error: 0, // sticky
};

const MAX_VISIBLE = 5;

const toasts = ref<ToastItem[]>([]);
const dismissTimers = new Map<string, ReturnType<typeof setTimeout>>();

export function dismissToast(id: string) {
  const timer = dismissTimers.get(id);
  if (timer !== undefined) {
    clearTimeout(timer);
    dismissTimers.delete(id);
  }
  toasts.value = toasts.value.filter((toast) => toast.id !== id);
}

export function pushToast(opts: {
  tone: ToastTone;
  title: string;
  description?: string;
  durationMs?: number;
}): string {
  // Over capacity: drop the oldest auto-dismissible toast, keep errors visible.
  if (toasts.value.length >= MAX_VISIBLE) {
    const victim = toasts.value.find((toast) => toast.tone !== 'error') ?? toasts.value[0];
    if (victim) dismissToast(victim.id);
  }

  const id = Date.now().toString(36) + Math.random().toString(36).substring(2, 6);
  toasts.value.push({
    id,
    tone: opts.tone,
    title: opts.title,
    description: opts.description,
  });

  const duration = opts.durationMs ?? AUTO_DISMISS_MS[opts.tone];
  if (duration > 0) {
    dismissTimers.set(
      id,
      setTimeout(() => dismissToast(id), duration)
    );
  }
  return id;
}

export function useToast() {
  return { toasts, pushToast, dismissToast };
}
