import { ref } from 'vue';

/**
 * Notification payload emitted by the Rust Host.
 */
export interface HostNotificationPayload {
  level: 'info' | 'warning' | 'error';
  msg: string;
  title?: string;
}

/**
 * Whether Host notifications are being collected instead of shown.
 *
 * NOTE: Module-level state is intentional — the UI enforces that only one
 * backup/restore operation can run at a time (via button locks), so
 * concurrent collection is not a concern.
 */
const collecting = ref(false);

/**
 * Collected Host notifications during an operation.
 */
const collected = ref<HostNotificationPayload[]>([]);

/**
 * Temporarily collects Host notifications (for example restore-time warnings)
 * so the caller can consolidate them into one result notification.
 */
export function useHostNotificationCollector() {
  function startCollecting() {
    collecting.value = true;
    collected.value = [];
  }

  function stopCollecting(): HostNotificationPayload[] {
    const result = [...collected.value];
    collecting.value = false;
    collected.value = [];
    return result;
  }

  function isCollecting(): boolean {
    return collecting.value;
  }

  function addIfCollecting(payload: HostNotificationPayload): boolean {
    if (collecting.value) {
      collected.value.push(payload);
      return true;
    }
    return false;
  }

  return {
    isCollecting,
    startCollecting,
    stopCollecting,
    addIfCollecting,
  };
}
