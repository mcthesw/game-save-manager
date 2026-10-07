import assert from 'node:assert/strict';
import test from 'node:test';
import { hasGameNameConflict, nextInstanceName } from './gameName.ts';

test('existing same-title games can keep their names while editing', () => {
  const first = { name: 'Same', storage_key: 'first' };
  const second = { name: 'Same', storage_key: 'second' };
  assert.equal(hasGameNameConflict([first, second], ' Same ', second), false);
  assert.equal(hasGameNameConflict([first, second], 'SAME', { ...second }), false);
});

test('new games and renames cannot take another games name', () => {
  const first = { name: 'Same', storage_key: 'first' };
  const second = { name: 'Other', storage_key: 'second' };
  assert.equal(hasGameNameConflict([first, second], ' same '), true);
  assert.equal(hasGameNameConflict([first, second], 'same', second), true);
  assert.equal(hasGameNameConflict([first, second], 'Unique', second), false);
});

test('additional instances get an unused name without changing existing titles', () => {
  assert.equal(nextInstanceName([{ name: 'GAME (2)' }, { name: 'Game (3)' }], 'Game'), 'Game (4)');
  assert.equal(nextInstanceName([{ name: 'My renamed game' }], 'Game'), 'Game (2)');
});
