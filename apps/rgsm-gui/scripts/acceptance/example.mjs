import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { waitForHttpHost } from '../wait-http-host.ts';
import { serveDevice } from './serve-device.mjs';
import { runBuild } from './build-task.mjs';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const repoRoot = resolve(appRoot, '../..');
const execute = promisify(execFile);
const viteCli = fileURLToPath(new URL('./bin/vite.js', import.meta.resolve('vite/package.json')));

export async function buildExample(session) {
  // Recheck source on each launch; Cargo reuses valid incremental output.
  console.log('Preparing HTTP host and built GUI (no desktop windows)');
  const options = {
    cwd: repoRoot,
    windowsHide: true,
    signal: session.signal,
    maxBuffer: 8 * 1024 * 1024,
  };
  await runBuild('cargo', ['build', '--locked', '-p', 'rgsm', '--bin', 'rgsm'], {
    ...options,
    logPath: join(session.logsDir, 'build.log'),
    env: { ...process.env, CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? '2' },
  });
  const outDir = join(session.root, 'web');
  await runBuild(process.execPath, [viteCli, 'build', '--outDir', outDir, '--logLevel', 'warn'], {
    ...options,
    cwd: appRoot,
    logPath: join(session.logsDir, 'web-build.log'),
  });
  session.signal.throwIfAborted();
  const target = resolve(repoRoot, process.env.CARGO_TARGET_DIR ?? 'target');
  const binary = join(target, 'debug', process.platform === 'win32' ? 'rgsm.exe' : 'rgsm');
  const revision = (await execute('git', ['rev-parse', 'HEAD'], options)).stdout.trim();
  const changes = (await execute('git', ['status', '--porcelain'], options)).stdout.trim();
  await writeFile(
    join(session.evidenceDir, 'runtime.json'),
    JSON.stringify(
      {
        revision,
        changes,
        binary,
        outDir,
        platform: process.platform,
        environment: 'HTTP-only host with built web GUI',
        startedAt: new Date().toISOString(),
      },
      null,
      2
    ) + '\n'
  );
  return { binary, outDir, appRoot, repoRoot };
}

export async function startExampleDevice(session, device, artifacts) {
  session.start(artifacts.binary, [], {
    cwd: artifacts.repoRoot,
    env: {
      ...process.env,
      RGSM_HTTP_HOST_ONLY: '1',
      RGSM_E2E_APP_DATA_DIR: device.appDataDir,
      RGSM_E2E_DEVICE_ID: device.id,
      RUST_LOG: 'info',
    },
  });
  const host = await waitForHttpHost(device.appDataDir, { signal: session.signal });
  return serveDevice(session, device, artifacts, host);
}
