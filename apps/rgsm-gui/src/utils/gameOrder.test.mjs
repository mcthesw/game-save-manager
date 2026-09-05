import assert from 'node:assert/strict';
import test from 'node:test';
import { applyGameOrder } from './gameOrder.ts';

test('draft order uses current definitions without resurrecting removed games', () => {
  const games = [
    { storage_key: 'a', name: 'Renamed' },
    { storage_key: 'b', name: 'Same name' },
    { storage_key: 'new', name: 'Same name' },
  ];
  assert.deepEqual(applyGameOrder(games, ['b', 'removed', 'a', 'b']), [
    games[1],
    games[0],
    games[2],
  ]);
  assert.deepEqual(applyGameOrder(games, []), games);
});
