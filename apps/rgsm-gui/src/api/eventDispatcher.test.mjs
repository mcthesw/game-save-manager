import assert from 'node:assert/strict';
import test from 'node:test';
import { createEventDispatcher } from './eventDispatcher.ts';

test('late subscribers receive latest pending state but not old notifications', () => {
  const dispatcher = createEventDispatcher(new Set(['pending']));
  dispatcher.dispatch({ eventType: 'pending', payload: ['old'] });
  dispatcher.dispatch({ eventType: 'pending', payload: ['current'] });
  dispatcher.dispatch({ eventType: 'notification', payload: 'once' });
  const seen = [];
  const stop = dispatcher.listen('pending', (event) => seen.push(event.payload));
  dispatcher.listen('notification', (event) => seen.push(event.payload));
  assert.deepEqual(seen, [['current']]);
  dispatcher.dispatch({ eventType: 'pending', payload: [] });
  stop();
  dispatcher.dispatch({ eventType: 'pending', payload: ['later'] });
  assert.deepEqual(seen, [['current'], []]);
});
