import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { waitForHttpOk } from './http-probe.ts';

async function serve(t, respond) {
  const server = createServer(respond);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  t.after(async () => {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  });
  return `http://127.0.0.1:${server.address().port}`;
}

test('a listener that never responds cannot bypass the startup deadline', async (t) => {
  const url = await serve(t, () => {});
  const result = waitForHttpOk(url, 100, 'hung listener').then(
    () => 'unexpected success',
    (error) => error.message
  );
  assert.match(
    await Promise.race([result, delay(600, 'still waiting')]),
    /Timed out.*hung listener/
  );
});

test('readiness retries a not-yet-ready response and accepts a later success', async (t) => {
  let reads = 0;
  const url = await serve(t, (_request, response) => {
    response.writeHead(++reads === 1 ? 503 : 200).end();
  });
  await waitForHttpOk(url, 2_000, 'starting listener');
  assert.equal(reads, 2);
});
