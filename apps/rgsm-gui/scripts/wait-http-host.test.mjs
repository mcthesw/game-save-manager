import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { once } from 'node:events';
import { waitForHttpHost } from './wait-http-host.ts';

async function fixture(t, respond) {
  const dir = await mkdtemp(join(tmpdir(), 'rgsm-host-ready-'));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const server = createServer(respond);
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  t.after(
    () =>
      new Promise((resolve) => {
        server.close(resolve);
        server.closeAllConnections();
      })
  );
  const port = server.address().port;
  await writeFile(
    join(dir, 'GameSaveManager.host.json'),
    JSON.stringify({ port, api_token: 'fixture-token' })
  );
  return { dir, port };
}

test('host readiness checks the discovered endpoint with authentication and retries', async (t) => {
  let requests = 0;
  const { dir, port } = await fixture(t, (req, res) => {
    assert.equal(req.url, '/api/v1/get-build-info');
    assert.equal(req.method, 'POST');
    assert.equal(req.headers.authorization, 'Bearer fixture-token');
    res.writeHead(++requests === 1 ? 503 : 200).end('{}');
  });
  assert.deepEqual(await waitForHttpHost(dir, { timeoutMs: 3_000 }), {
    apiBaseUrl: `http://127.0.0.1:${port}`,
    token: 'fixture-token',
    port,
  });
  assert.equal(requests, 2);
});

test('cancellation interrupts an in-flight readiness request', async (t) => {
  const controller = new AbortController();
  const { dir } = await fixture(t, () => controller.abort(new Error('setup cancelled')));
  await assert.rejects(waitForHttpHost(dir, { signal: controller.signal }), /setup cancelled/);
});

test('hung requests respect the overall timeout and process failures stop polling', async (t) => {
  const { dir } = await fixture(t, () => {});
  await assert.rejects(waitForHttpHost(dir, { timeoutMs: 100 }), /Timed out/);
  await assert.rejects(
    waitForHttpHost(dir, { failure: () => new Error('host exited') }),
    /host exited/
  );
});
