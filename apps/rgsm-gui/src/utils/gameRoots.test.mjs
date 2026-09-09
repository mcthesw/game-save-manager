import assert from 'node:assert/strict';
import test from 'node:test';
import { gameRootPathKey, mergeDuplicateGameRoots, newGameRootPaths } from './gameRoots.ts';

test('Steam detection compares Windows path identity and deduplicates the incoming batch', () => {
  assert.deepEqual(
    newGameRootPaths(
      ['c:\\program files (x86)\\steam'],
      ['C:/Program Files (x86)/Steam/', 'D:/Games', 'd:\\games\\']
    ),
    ['D:/Games']
  );
});

test('Unix root paths remain case-sensitive', () => {
  assert.deepEqual(newGameRootPaths(['/games'], ['/games/', '/Games']), ['/Games']);
  assert.notEqual(gameRootPathKey('c:'), gameRootPathKey('C:/'));
  assert.equal(gameRootPathKey('/'), '/');
});

test('merging roots preserves IDs, bindings, installations, other stores and empty drafts', () => {
  const root = (id, path, store = 'steam') => ({
    id,
    source: 'manual',
    kind: { type: 'gameRoot', store, path },
  });
  const device = {
    id: 'pc',
    next_resource_id: 20,
    resources: [
      root(2, 'C:/Steam'),
      root(5, 'c:\\steam\\'),
      root(8, 'C:/Steam', 'other'),
      root(9, ''),
      root(10, ''),
      {
        id: 12,
        kind: { type: 'gameInstallation', root_id: 5, path: 'C:/Steam/steamapps/common/Test' },
      },
    ],
  };
  const games = [{ device_bindings: { pc: { rootIds: [5, 2, 8] }, other: { rootIds: [5] } } }];
  mergeDuplicateGameRoots(device, games);
  assert.deepEqual(
    device.resources.map((resource) => resource.id),
    [2, 8, 9, 10, 12]
  );
  assert.equal(device.next_resource_id, 20);
  assert.equal(device.resources.at(-1).kind.root_id, 2);
  assert.deepEqual(games[0].device_bindings.pc.rootIds, [2, 8]);
  assert.deepEqual(games[0].device_bindings.other.rootIds, [5]);
  const once = structuredClone({ device, games });
  mergeDuplicateGameRoots(device, games);
  assert.deepEqual({ device, games }, once);
});
