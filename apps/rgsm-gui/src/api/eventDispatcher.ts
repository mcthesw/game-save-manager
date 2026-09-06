/** Stateful SSE messages must also reach components mounted after connection starts. */
export function createEventDispatcher<Event extends { eventType: string }>(stateful: Set<string>) {
  const latest = new Map<string, Event>();
  const listeners = new Map<string, Set<(event: Event) => void>>();
  return {
    dispatch(event: Event) {
      if (stateful.has(event.eventType)) latest.set(event.eventType, event);
      listeners.get(event.eventType)?.forEach((listener) => listener(event));
    },
    listen(type: string, listener: (event: Event) => void) {
      let subscriptions = listeners.get(type);
      if (!subscriptions) {
        subscriptions = new Set();
        listeners.set(type, subscriptions);
      }
      subscriptions.add(listener);
      const previous = latest.get(type);
      if (previous) listener(previous);
      return () => {
        subscriptions.delete(listener);
      };
    },
  };
}
