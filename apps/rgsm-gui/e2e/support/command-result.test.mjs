import assert from 'node:assert/strict';
import test from 'node:test';
import { waitForCommand } from './command-result.ts';

function response(status, date = 'selected') {
  return {
    url: () => 'http://127.0.0.1:1234/api/v1/delete-snapshot',
    request: () => ({ method: () => 'POST', postDataJSON: () => ({ date }) }),
    ok: () => status === 200,
    status: () => status,
    text: async () => (status === 200 ? 'null' : 'catalog access denied'),
  };
}

test('registers response observation before clicking and ignores another snapshot', async () => {
  let receive;
  let matches;
  const page = {
    waitForResponse: (predicate) => {
      matches = predicate;
      return new Promise((resolve) => {
        receive = resolve;
      });
    },
  };
  let read = false;
  const selected = response(200);
  selected.text = async () => {
    read = true;
    return 'null';
  };
  await waitForCommand(page, '/api/v1/delete-snapshot', 'selected', async () => {
    assert.ok(receive);
    assert.equal(matches(response(200, 'older')), false);
    assert.equal(matches(selected), true);
    receive(selected);
  });
  assert.equal(read, true);
});

test('a failed current request cannot be mistaken for a previous successful notification', async () => {
  const page = { waitForResponse: async () => response(400) };
  await assert.rejects(
    waitForCommand(page, '/api/v1/delete-snapshot', 'selected', async () => {}),
    /delete-snapshot failed \(400\): catalog access denied/
  );
});

test('a failed click is propagated instead of waiting for a request that was never sent', async () => {
  const page = { waitForResponse: () => new Promise(() => {}) };
  await assert.rejects(
    waitForCommand(page, '/api/v1/delete-snapshot', 'selected', async () => {
      throw new Error('click failed');
    }),
    /click failed/
  );
});
