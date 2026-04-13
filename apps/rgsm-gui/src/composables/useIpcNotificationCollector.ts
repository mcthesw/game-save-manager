import { ref } from 'vue';

/**
 * IPC notification payload matching the Tauri backend `IpcNotification` event.
 */
export interface IpcNotificationPayload {
  level: 'info' | 'warning' | 'error';
  msg: string;
  title?: string;
}

/**
 * Whether IPC notifications are being collected instead of shown.
 *
 * NOTE: Module-level state is intentional — the UI enforces that only one
 * backup/restore operation can run at a time (via button locks), so
 * concurrent collection is not a concern.
 */
const collecting = ref(false);

/**
 * Collected IPC notifications during an operation.
 */
const collected = ref<IpcNotificationPayload[]>([]);

/**
 * Composable that allows temporarily collecting IPC notifications
 * (e.g. restore-time warnings) instead of displaying them immediately,
 * so they can be consolidated into a single result notification.
 */
export function useIpcNotificationCollector() {
  function startCollecting() {
    collecting.value = true;
    collected.value = [];
  }

  function stopCollecting(): IpcNotificationPayload[] {
    const result = [...collected.value];
    collecting.value = false;
    collected.value = [];
    return result;
  }

  function isCollecting(): boolean {
    return collecting.value;
  }

  function addIfCollecting(payload: IpcNotificationPayload): boolean {
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
