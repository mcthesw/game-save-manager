import assert from 'node:assert/strict';
import test from 'node:test';
import { quickActionAffectsGame } from './quickActionRefresh.ts';

const game = { storage_key: 'key-1' };
const event = (overrides) => ({
  operation: 'Backup',
  status: 'Success',
  trigger: 'Hotkey',
  game_id: 'key-1',
  game_name: 'Same title',
  ...overrides,
});

test('refreshes after a backup or restore, including failures', () => {
  assert.equal(quickActionAffectsGame(event({}), game), true);
  assert.equal(quickActionAffectsGame(event({ operation: 'Apply' }), game), true);
  assert.equal(quickActionAffectsGame(event({ status: 'Failure' }), game), true);
});

test('ignores unchanged backups and other games with the same title', () => {
  assert.equal(quickActionAffectsGame(event({ status: 'SkippedUnchanged' }), game), false);
  assert.equal(quickActionAffectsGame(event({ game_id: 'key-2' }), game), false);
  assert.equal(quickActionAffectsGame(event({ game_id: null }), game), false);
});
