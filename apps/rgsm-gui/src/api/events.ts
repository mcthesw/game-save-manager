import './client';
import { streamEvents } from './generated/sdk.gen';
import { createEventDispatcher } from './eventDispatcher';
import type {
  CloudSyncErrorEvent,
  CloudSyncStatusEvent,
  HostEvent,
  HostNotification,
  QuickActionCompleted,
  PendingProgress,
} from './generated/types.gen';

export type { CloudSyncErrorEvent, CloudSyncStatusEvent, HostNotification, QuickActionCompleted };

type HostEventMap = {
  'cloud-sync-error': CloudSyncErrorEvent;
  'cloud-sync-status': CloudSyncStatusEvent;
  notification: HostNotification;
  'quick-action-completed': QuickActionCompleted;
  'remote-progress-pending': PendingProgress;
};

type Listener<T> = (event: { payload: T }) => void;
const dispatcher = createEventDispatcher<HostEvent>(
  new Set(['cloud-sync-status', 'remote-progress-pending'])
);
let connection: AbortController | undefined;

async function connect(signal: AbortSignal) {
  while (!signal.aborted) {
    try {
      const { stream } = await streamEvents({
        signal,
        onSseError(error) {
          if (!signal.aborted) console.warn('RGSM event stream disconnected', error);
        },
      });
      for await (const event of stream) {
        dispatcher.dispatch(event as HostEvent);
      }
    } catch (error) {
      if (!signal.aborted) console.warn('RGSM event stream disconnected', error);
    }
    if (!signal.aborted) {
      await new Promise((resolve) => setTimeout(resolve, 500));
    }
  }
}

function listen<K extends keyof HostEventMap>(eventType: K, listener: Listener<HostEventMap[K]>) {
  const stop = dispatcher.listen(eventType, (event) =>
    listener({ payload: event.payload as HostEventMap[K] })
  );
  if (!connection) {
    connection = new AbortController();
    void connect(connection.signal);
  }
  return Promise.resolve(stop);
}

export const events = {
  remoteProgressPending: {
    listen: (listener: Listener<PendingProgress>) => listen('remote-progress-pending', listener),
  },
  cloudSyncErrorEvent: {
    listen: (listener: Listener<CloudSyncErrorEvent>) => listen('cloud-sync-error', listener),
  },
  cloudSyncStatusEvent: {
    listen: (listener: Listener<CloudSyncStatusEvent>) => listen('cloud-sync-status', listener),
  },
  ipcNotification: {
    listen: (listener: Listener<HostNotification>) => listen('notification', listener),
  },
  quickActionCompleted: {
    listen: (listener: Listener<QuickActionCompleted>) =>
      listen('quick-action-completed', listener),
  },
};
