import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn, spawnSync } from 'node:child_process';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const workspaceRoot = resolve(appRoot, '../..');
const hostConfigPath = resolve(workspaceRoot, '.rgsm-dev/app-data/GameSaveManager.host.json');
const viteCommand = process.platform === 'win32' ? 'cmd.exe' : 'pnpm';
const viteArgs =
  process.platform === 'win32' ? ['/d', '/s', '/c', 'pnpm.cmd exec vite'] : ['exec', 'vite'];

const host = spawn('cargo', ['run', '--locked', '-p', 'rgsm'], {
  cwd: workspaceRoot,
  env: { ...process.env, RGSM_HTTP_HOST_ONLY: '1' },
  stdio: 'inherit',
});
let vite;

let stopping = false;
function stopAll() {
  if (stopping) return;
  stopping = true;
  for (const child of [vite, host]) {
    if (!child) continue;
    if (child.exitCode !== null) continue;
    if (process.platform === 'win32' && child.pid) {
      spawnSync('taskkill', ['/pid', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    } else {
      child.kill('SIGTERM');
    }
  }
}

async function waitForHost() {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    if (host.exitCode !== null) {
      throw new Error(`RGSM Host exited with code ${host.exitCode}`);
    }
    try {
      const config = JSON.parse(await readFile(hostConfigPath, 'utf8'));
      const response = await fetch(`http://127.0.0.1:${config.port}/api/v1/get-build-info`, {
        method: 'POST',
        headers: { Authorization: `Bearer ${config.api_token}` },
      });
      if (response.ok) return;
    } catch {
      // Host is still starting.
    }
    const delay = Promise.withResolvers();
    setTimeout(delay.resolve, 250);
    await delay.promise;
  }
  throw new Error('Timed out waiting for RGSM Host');
}

try {
  await waitForHost();
  vite = spawn(viteCommand, viteArgs, {
    cwd: appRoot,
    stdio: 'inherit',
  });
  const viteExit = Promise.withResolvers();
  vite.once('error', viteExit.reject);
  vite.once('exit', (code) => viteExit.resolve(code ?? 1));
  const exitCode = await viteExit.promise;
  stopAll();
  process.exitCode = exitCode;
} catch (error) {
  stopAll();
  console.error(error);
  process.exitCode = 1;
}

for (const signal of ['SIGINT', 'SIGTERM']) {
  process.once(signal, () => {
    stopAll();
    process.exit(0);
  });
}
