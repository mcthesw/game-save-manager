import assert from 'node:assert/strict';
import test from 'node:test';
import { runUndoRestore } from './undoRestore.ts';

test('undo succeeds only after files and position are restored in order', async () => {
  const calls = [];
  const result = await runUndoRestore(
    async () => {
      calls.push('files');
      return { status: 'ok' };
    },
    async () => {
      calls.push('position');
      return { status: 'ok' };
    }
  );
  assert.deepEqual(calls, ['files', 'position']);
  assert.deepEqual(result, { status: 'ok' });
});

for (const stage of ['files', 'position']) {
  for (const throws of [false, true]) {
    test(`undo identifies ${stage} failure (${throws ? 'exception' : 'response'})`, async () => {
      const calls = [];
      const error = new Error('Access denied');
      const step = (name) => async () => {
        calls.push(name);
        if (name !== stage) return { status: 'ok' };
        if (throws) throw error;
        return { status: 'error', error };
      };
      assert.deepEqual(await runUndoRestore(step('files'), step('position')), {
        status: 'error',
        stage,
        error,
      });
      assert.deepEqual(calls, stage === 'files' ? ['files'] : ['files', 'position']);
    });
  }
}
