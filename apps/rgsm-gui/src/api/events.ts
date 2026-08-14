import './client';
import { streamEvents } from './generated/sdk.gen';
import type {
  CloudSyncErrorEvent,
  CloudSyncStatusEvent,
  HostEvent,
  HostNotification,
  QuickActionCompleted,
} from './generated/types.gen';

export type { CloudSyncErrorEvent, CloudSyncStatusEvent, HostNotification, QuickActionCompleted };

type HostEventMap = {
  'cloud-sync-error': CloudSyncErrorEvent;
  'cloud-sync-status': CloudSyncStatusEvent;
  notification: HostNotification;
  'quick-action-completed': QuickActionCompleted;
};

type Listener<T> = (event: { payload: T }) => void;
const listeners = new Map<keyof HostEventMap, Set<Listener<never>>>();
let connection: AbortController | undefined;

function dispatch(event: HostEvent) {
  const eventType = event.eventType as keyof HostEventMap;
  const payload = event.payload as HostEventMap[keyof HostEventMap];
  const subscriptions = listeners.get(eventType);
  subscriptions?.forEach((listener) => listener({ payload } as never));
}

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
        dispatch(event as HostEvent);
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
  let subscriptions = listeners.get(eventType);
  if (!subscriptions) {
    subscriptions = new Set();
    listeners.set(eventType, subscriptions);
  }
  subscriptions.add(listener as Listener<never>);
  if (!connection) {
    connection = new AbortController();
    void connect(connection.signal);
  }
  return Promise.resolve(() => {
    subscriptions?.delete(listener as Listener<never>);
  });
}

export const events = {
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
