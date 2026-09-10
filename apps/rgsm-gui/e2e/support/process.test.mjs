import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { closeResources, hostCommand, spawnTestProcess } from './process.ts';

test('resource cleanup runs in reverse acquisition order even after a close fails', async () => {
  const calls = [];
  const closers = [
    async () => calls.push('host'),
    async () => {
      calls.push('context');
      throw new Error('close failed');
    },
    async () => calls.push('page'),
  ];
  await assert.rejects(closeResources(closers), {
    name: 'AggregateError',
    message: /close failed/,
  });
  assert.deepEqual(calls, ['page', 'context', 'host']);
  await closeResources(closers);
  assert.equal(calls.length, 3);
});

test('a missing executable is reported without an unhandled process error', async () => {
  const owned = spawnTestProcess('rgsm-e2e-nonexistent-executable', []);
  await delay(100);
  assert.match(owned.failure().message, /ENOENT/);
  await owned.stop();
});

test('the owner stops only its child and repeated cleanup is harmless', async (t) => {
  const owned = spawnTestProcess(process.execPath, ['-e', 'setInterval(() => {}, 1000)']);
  t.after(() => owned.stop());
  await delay(100);
  assert.equal(owned.failure(), undefined);
  await Promise.all([owned.stop(), owned.stop()]);
  assert.ok(owned.child.exitCode !== null || owned.child.signalCode !== null);
});

test('Linux host launch isolates the session bus; other platforms use the binary directly', () => {
  assert.deepEqual(hostCommand('/test/rgsm', 'linux'), {
    command: 'dbus-run-session',
    args: ['--', '/test/rgsm'],
  });
  assert.deepEqual(hostCommand('D:/test/rgsm.exe', 'win32'), {
    command: 'D:/test/rgsm.exe',
    args: [],
  });
  assert.deepEqual(hostCommand('/test/rgsm', 'darwin'), { command: '/test/rgsm', args: [] });
});
