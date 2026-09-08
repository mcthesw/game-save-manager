import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';
import { runBuild } from './build-task.mjs';

test('one-shot builds capture output and reject failure', async (t) => {
  const dir = await mkdtemp(join(tmpdir(), 'rgsm-build-task-'));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const options = { signal: new AbortController().signal, logPath: join(dir, 'build.log') };
  await runBuild(process.execPath, ['-e', 'console.log("built")'], options);
  assert.equal((await readFile(options.logPath, 'utf8')).trim(), 'built');
  await assert.rejects(
    runBuild(process.execPath, ['-e', 'process.exit(2)'], options),
    /Build exited \(2\)/
  );
});

test(
  'cancelled Windows builds stop their subprocess tree before returning',
  {
    skip: process.platform !== 'win32' && 'Windows compiler subprocess cleanup',
  },
  async (t) => {
    const dir = await mkdtemp(join(tmpdir(), 'rgsm-build-cancel-'));
    t.after(() => rm(dir, { recursive: true, force: true }));
    const logPath = join(dir, 'build.log');
    const controller = new AbortController();
    const task = runBuild(
      process.execPath,
      [
        '-e',
        `
    const { spawn } = require('node:child_process');
    const child = spawn(process.execPath, ['-e', 'setInterval(()=>{},1000)'], { windowsHide: true, stdio: 'ignore' });
    console.log(child.pid);
    setInterval(()=>{},1000);
  `,
      ],
      { signal: controller.signal, logPath }
    );
    const rejected = assert.rejects(task, { name: 'AbortError' });
    try {
      let pid;
      const deadline = Date.now() + 5_000;
      while (!pid && Date.now() < deadline) {
        pid = Number(await readFile(logPath, 'utf8').catch(() => ''));
        if (!pid) await delay(20);
      }
      assert.ok(pid, 'child must have started before cancellation');
      controller.abort();
      await rejected;
      assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
    } finally {
      controller.abort();
      await rejected;
    }
  }
);
